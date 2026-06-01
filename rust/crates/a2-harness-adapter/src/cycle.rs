//! One harness assertion cycle.
//!
//! `run_cycle` is the library entry point. It:
//!
//! 1. validates the caller config (refusal => STOP cycle, no invoke);
//! 2. classifies the workspace (non-disposable refused => STOP cycle);
//! 3. invokes `claw plan status` once (or `repeat_invocations` times
//!    for idempotency assertions);
//! 4. parses each envelope, flagging schema drift as STOP;
//! 5. runs caller assertions;
//! 6. emits a structured report at full envelope fidelity.

use crate::classifier::{classify_workspace, ClassifierConfig};
use crate::config::{ConfigError, ExpectedOutcome, HarnessAssertionConfig};
use crate::envelope::{
    classify_next_operator_command, parse_envelope, EnvelopeParseError, NextOpCommandShape,
    EXIT_STATUS_REFUSED, REFUSED_AUDIT_MARKER,
};
use crate::invoker::StatusInvoker;
use crate::report::{AssertionEntry, CycleClassification, HarnessRunReport, InvocationRecord};
use crate::stop::{phase_is_stop_bearing, StopKind, StopSignal};

/// Cycle entry-point errors. These are exposed for callers that want
/// to distinguish setup failures from the cycle's STOP signals; the
/// harness's own report carries every STOP signal in `stop_signals`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycleError {
    Config(ConfigError),
    Io(String),
}

/// Run one assertion cycle. The classifier and invoker are injected so
/// tests can substitute mocks for both.
///
/// # Errors
///
/// Returns [`CycleError::Config`] when the caller config is refused,
/// and [`CycleError::Io`] when the status subprocess fails to spawn or
/// capture. Both cases also surface as STOP signals in the harness
/// report when the report path is taken; this return type is for
/// callers that want a typed error.
#[allow(clippy::too_many_lines)]
pub fn run_cycle<I: StatusInvoker>(
    cfg: &HarnessAssertionConfig,
    classifier_cfg: &ClassifierConfig,
    invoker: &I,
) -> Result<HarnessRunReport, CycleError> {
    // Step 1: validate config. A refused config never spawns anything.
    if let Err(e) = cfg.validate() {
        return Ok(refusal_report(
            cfg,
            classifier_cfg,
            CycleClassification::Stop,
            format!("config refused: {e:?}"),
            vec![config_error_to_stop(&e)],
        ));
    }

    // Step 2: classify workspace. Non-disposable + no auth => STOP.
    let classifier_decision = classify_workspace(&cfg.workspace_root, classifier_cfg);
    if classifier_decision.is_refused() {
        return Ok(refusal_report(
            cfg,
            classifier_cfg,
            CycleClassification::Stop,
            format!(
                "non-disposable workspace refused at `{}`",
                cfg.workspace_root.display()
            ),
            vec![StopSignal::new(StopKind::NonDisposableWorkspaceRefused(
                cfg.workspace_root.to_string_lossy().into_owned(),
            ))],
        ));
    }

    // Step 3-5: invoke subprocess (or paired for idempotency) and
    // parse + assert.
    let mut invocations: Vec<InvocationRecord> = Vec::new();
    let mut assertions: Vec<AssertionEntry> = Vec::new();
    let mut stops: Vec<StopSignal> = Vec::new();

    let n = u32::from(cfg.repeat_invocations);
    let mut raw_stdouts: Vec<Vec<u8>> = Vec::with_capacity(n as usize);

    for _ in 0..n {
        let inv = match invoker.invoke(&cfg.workspace_root, cfg.approval_result_path.as_deref()) {
            Ok(i) => i,
            Err(e) => {
                return Err(CycleError::Io(format!("invoke failed: {e}")));
            }
        };

        let argv_strs: Vec<String> = inv
            .argv
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let stdout = inv.stdout.clone();
        let exit_code = inv.exit_code;
        raw_stdouts.push(stdout.clone());

        let mut envelope_parsed = None;
        match parse_envelope(&stdout) {
            Ok(env) => {
                envelope_parsed = Some(env.clone());
                run_envelope_assertions(cfg, &env, &mut assertions, &mut stops);
                if exit_code == EXIT_STATUS_REFUSED {
                    stops.push(StopSignal::new(StopKind::ProducerRefused));
                    if !env.audit_markers.iter().any(|m| m == REFUSED_AUDIT_MARKER) {
                        stops.push(StopSignal::new(StopKind::ExitRefusedMissingMarker {
                            observed_markers: env.audit_markers.clone(),
                        }));
                    }
                }
            }
            Err(err) => {
                stops.push(parse_error_to_stop(&err));
            }
        }

        invocations.push(InvocationRecord {
            argv: argv_strs,
            stdout_raw: stdout,
            exit_code,
            envelope_parsed,
        });
    }

    // Step 5b: idempotency byte-comparison when paired reads requested.
    if cfg.repeat_invocations >= 2 {
        let first = &raw_stdouts[0];
        let all_equal = raw_stdouts.iter().all(|s| s == first);
        let entry = AssertionEntry {
            name: "idempotency.byte_identical_paired_stdout".to_string(),
            expected: "all paired stdouts byte-identical".to_string(),
            observed: if all_equal {
                "byte-identical".to_string()
            } else {
                "non-byte-identical".to_string()
            },
            passed: all_equal,
        };
        assertions.push(entry);
        if !all_equal {
            stops.push(StopSignal::new(StopKind::IdempotencyMismatch));
        }
    }

    // Compute final classification.
    let classification = if !stops.is_empty() {
        CycleClassification::Stop
    } else if assertions.iter().any(|a| !a.passed) {
        CycleClassification::Fail
    } else {
        CycleClassification::Pass
    };

    let diagnostic = build_diagnostic(classification, &assertions, &stops);

    Ok(HarnessRunReport {
        classification,
        diagnostic,
        classifier_decision,
        invocations,
        assertions,
        stop_signals: stops,
    })
}

#[allow(clippy::too_many_lines)]
fn run_envelope_assertions(
    cfg: &HarnessAssertionConfig,
    env: &crate::envelope::StatusEnvelope,
    assertions: &mut Vec<AssertionEntry>,
    stops: &mut Vec<StopSignal>,
) {
    // Always check `next_operator_command` shape; unknown shapes are
    // STOP signals regardless of caller expectation.
    let shape = classify_next_operator_command(&env.next_operator_command);
    if matches!(shape, NextOpCommandShape::Unknown) {
        stops.push(StopSignal::new(StopKind::UnknownEnumLiteral {
            field: "next_operator_command",
            value: env.next_operator_command.clone(),
        }));
    }

    // Phase assertion (when caller declared an expected phase).
    if let Some(expected_phase) = cfg.expected_phase {
        let passed = env.phase == expected_phase;
        assertions.push(AssertionEntry {
            name: "envelope.phase".to_string(),
            expected: format!("{expected_phase:?}"),
            observed: format!("{:?}", env.phase),
            passed,
        });
    }

    // STOP-bearing-phase detection (independent of caller expectation).
    if phase_is_stop_bearing(env.phase) {
        stops.push(StopSignal::new(StopKind::StopBearingPhase(env.phase)));
    }

    // Producer stop_condition.
    if let Some(sc) = env.stop_condition {
        stops.push(StopSignal::new(StopKind::ProducerStopCondition(sc)));
        // Producer always populates at least one evidence path when a
        // STOP fires. An empty `evidence_paths` under a non-null
        // `stop_condition` is producer-broken drift; the harness
        // raises an additional STOP in its own right.
        if env.evidence_paths.is_empty() {
            stops.push(StopSignal::new(
                StopKind::EvidencePathsEmptyUnderStopCondition(sc),
            ));
        }
    }

    // Producer STOP — escalate next-op string (verbatim literal).
    if env.next_operator_command == "STOP — escalate" {
        stops.push(StopSignal::new(StopKind::ProducerStopEscalate));
    }

    // Evidence-path substring assertions.
    for needle in &cfg.expected_evidence_substrings {
        let any_match = env.evidence_paths.iter().any(|p| p.contains(needle));
        assertions.push(AssertionEntry {
            name: format!("evidence_paths.contains.`{needle}`"),
            expected: format!("at least one path containing `{needle}`"),
            observed: env.evidence_paths.join(", "),
            passed: any_match,
        });
    }

    // Outcome assertion (continue vs STOP, and which STOP).
    let observed_stop = stops.iter().any(|s| {
        matches!(
            s.kind,
            StopKind::ProducerStopCondition(_)
                | StopKind::StopBearingPhase(_)
                | StopKind::ProducerStopEscalate
                | StopKind::UnknownEnumLiteral { .. }
        )
    });
    match &cfg.expected_outcome {
        ExpectedOutcome::Continue => {
            let passed = !observed_stop;
            assertions.push(AssertionEntry {
                name: "outcome.expected_continue".to_string(),
                expected: "no STOP signal".to_string(),
                observed: if observed_stop {
                    "STOP observed".to_string()
                } else {
                    "no STOP".to_string()
                },
                passed,
            });
            if observed_stop {
                stops.push(StopSignal::new(StopKind::ExpectedContinueObservedStop));
            }
        }
        ExpectedOutcome::Stop { stop_condition } => {
            if !observed_stop {
                assertions.push(AssertionEntry {
                    name: "outcome.expected_stop".to_string(),
                    expected: "STOP signal".to_string(),
                    observed: "no STOP".to_string(),
                    passed: false,
                });
                stops.push(StopSignal::new(StopKind::ExpectedStopObservedContinue));
            } else if let Some(expected_sc) = stop_condition {
                if env.stop_condition == Some(*expected_sc) {
                    assertions.push(AssertionEntry {
                        name: "outcome.expected_stop.stop_condition".to_string(),
                        expected: format!("{expected_sc:?}"),
                        observed: format!("{:?}", env.stop_condition),
                        passed: true,
                    });
                } else {
                    let expected_label = format!("{expected_sc:?}");
                    let observed_label = format!("{:?}", env.stop_condition);
                    assertions.push(AssertionEntry {
                        name: "outcome.expected_stop.stop_condition".to_string(),
                        expected: expected_label.clone(),
                        observed: observed_label.clone(),
                        passed: false,
                    });
                    stops.push(StopSignal::new(StopKind::WrongStopValue {
                        expected: expected_label,
                        observed: observed_label,
                    }));
                }
            } else {
                assertions.push(AssertionEntry {
                    name: "outcome.expected_stop".to_string(),
                    expected: "STOP signal (any)".to_string(),
                    observed: "STOP".to_string(),
                    passed: true,
                });
            }
        }
        ExpectedOutcome::Unasserted => {}
    }
}

fn parse_error_to_stop(err: &EnvelopeParseError) -> StopSignal {
    let kind = match err {
        EnvelopeParseError::InvalidJson(s) => StopKind::InvalidJson(s.clone()),
        EnvelopeParseError::SchemaDrift(s) => StopKind::SchemaDrift(s.clone()),
        EnvelopeParseError::SchemaVersionMismatch(s) => StopKind::SchemaVersionMismatch(s.clone()),
        EnvelopeParseError::ReadOnlyInvariantAltered(s) => {
            StopKind::ReadOnlyInvariantAltered(s.clone())
        }
        EnvelopeParseError::NextOpCommandUnknown(s) => StopKind::UnknownEnumLiteral {
            field: "next_operator_command",
            value: s.clone(),
        },
    };
    StopSignal::new(kind)
}

fn config_error_to_stop(err: &ConfigError) -> StopSignal {
    let kind = match err {
        ConfigError::ChainWriteSubcommandReferenced(s) => {
            StopKind::ConfigReferencedChainWriteCommand(s.clone())
        }
        _ => StopKind::ConfigReferencedChainWriteCommand(format!("{err:?}")),
    };
    StopSignal::new(kind)
}

fn refusal_report(
    cfg: &HarnessAssertionConfig,
    classifier_cfg: &ClassifierConfig,
    classification: CycleClassification,
    diagnostic: String,
    stops: Vec<StopSignal>,
) -> HarnessRunReport {
    HarnessRunReport {
        classification,
        diagnostic,
        classifier_decision: classify_workspace(&cfg.workspace_root, classifier_cfg),
        invocations: Vec::new(),
        assertions: Vec::new(),
        stop_signals: stops,
    }
}

fn build_diagnostic(
    classification: CycleClassification,
    assertions: &[AssertionEntry],
    stops: &[StopSignal],
) -> String {
    let n_pass = assertions.iter().filter(|a| a.passed).count();
    let n_fail = assertions.iter().filter(|a| !a.passed).count();
    let n_stop = stops.len();
    format!(
        "cycle: {classification:?}; assertions {n_pass}/{} passed; {n_fail} failed; {n_stop} STOP signal(s)",
        assertions.len()
    )
}
