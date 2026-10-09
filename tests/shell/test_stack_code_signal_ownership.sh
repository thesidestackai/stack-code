#!/usr/bin/env bash
# Retains evidence. All children/providers are disposable fakes; Cargo is offline.
set -euo pipefail
REPO="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
: "${R4_EVIDENCE_DIR:?set a new disposable evidence directory}"
python3 - "$REPO" "$R4_EVIDENCE_DIR" <<'PY'
from pathlib import Path
import fcntl, hashlib, json, os, pty, select, shutil, signal, subprocess, sys, termios, time
repo, out = map(Path, sys.argv[1:])
out.mkdir(parents=True, exist_ok=False)
fixture = out/'fixture'; (fixture/'src').mkdir(parents=True)
main = (repo/'rust/crates/rusty-claude-cli/src/main.rs').read_text()
start = main.index('struct HookAbortMonitor {')
monitor = main[start:main.index('\nimpl LiveCli {', start)]
start = main.index('fn external_integrations_disabled()')
guard = main[start:main.index('\n}', start)+2]
assert 'HookAbortMonitor::spawn(hook_abort_signal)' in main
(fixture/'src/monitor.rs').write_text(monitor+'\n'+guard+'\n')
for src, dst in [('tests/fixtures/r4_signal.rs','main.rs'), ('rust/crates/rusty-claude-cli/src/input.rs','input.rs')]:
    shutil.copyfile(repo/src, fixture/'src'/dst)
(fixture/'Cargo.toml').write_text(f'''[package]
name="r4-signal-fixture"
version="0.0.0"
edition="2021"
[dependencies]
runtime={{path="{repo}/rust/crates/runtime"}}
tools={{path="{repo}/rust/crates/tools"}}
tokio={{version="1",features=["rt","macros","signal","sync"]}}
rustyline="15"
''')
shutil.copyfile(repo/'rust/Cargo.lock', fixture/'Cargo.lock')
env = dict(os.environ, CARGO_NET_OFFLINE='true')
env['CARGO_TARGET_DIR'] = os.environ.get('R4_TARGET_DIR', str(out/'target'))
with (out/'build.log').open('w') as log:
    # The standalone root adds one lock entry; dependency identities are checked below.
    subprocess.run(['cargo','build','--offline','--manifest-path',str(fixture/'Cargo.toml')],env=env,stdout=log,stderr=log,check=True)
import tomllib
base = {(p['name'],p['version'],p.get('source')):p.get('checksum') for p in tomllib.loads((repo/'rust/Cargo.lock').read_text())['package']}
for p in tomllib.loads((fixture/'Cargo.lock').read_text())['package']:
    if p['name'] != 'r4-signal-fixture':
        key = (p['name'],p['version'],p.get('source'))
        assert key in base and p.get('checksum') == base[key], key
binary = Path(env['CARGO_TARGET_DIR'])/'debug/r4-signal-fixture'
rows=[]
def run(stage, trigger, text=b'', expect_signal=True):
    case = f'{stage}-{trigger}-{len(rows)}'; root=out/case; root.mkdir(); (root/'marker').write_text('unchanged')
    master=None; log=(root/'output.log').open('wb')
    if trigger in ('pty','eof') or stage=='idle':
        master, slave=pty.openpty()
        def setup():
            os.setsid(); fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        p=subprocess.Popen([str(binary),str(root),stage],stdin=slave,stdout=slave,stderr=slave,preexec_fn=setup,cwd=root,env=dict(env,TERM='xterm-256color'))
        os.close(slave)
    else:
        p=subprocess.Popen([str(binary),str(root),stage],stdin=subprocess.DEVNULL,stdout=log,stderr=log,start_new_session=True,cwd=root,env=env)
    tty=b''
    def events(): return (root/'events').read_text().splitlines() if (root/'events').exists() else []
    def drain():
        nonlocal tty
        if master is not None and select.select([master],[],[],0)[0]:
            try: tty+=os.read(master,65536)
            except OSError: pass
    ready='idle-ready' if stage=='idle' else f'ready:{stage}'
    deadline=time.monotonic()+10
    while p.poll() is None and time.monotonic()<deadline:
        drain()
        if stage=='normal' or (ready in events() and (stage!='idle' or b'\x1b[?2004h' in tty)): break
        time.sleep(.005)
    assert stage=='normal' or ready in events(), (case,p.poll(),events(),tty)
    if stage=='idle': assert b'\x1b[?2004h' in tty, (case,tty)
    before=events(); marker=(root/'marker').read_bytes(); signal_sent=False; second_signal_sent=False
    if expect_signal:
        assert os.getpgid(p.pid)==p.pid
        if text: os.write(master,text); time.sleep(.03)
        if trigger=='pty': os.write(master,b'\x03')
        elif trigger=='group': os.killpg(p.pid,signal.SIGINT)
        else: p.send_signal(signal.SIGINT)
        # Rustyline consumes raw-mode Ctrl-C as a key, not a kernel signal.
        signal_sent = not (trigger=='pty' and stage=='idle')
        time.sleep(.05)
        if trigger=='repeat' and p.poll() is None:
            p.send_signal(signal.SIGINT); second_signal_sent=True
        (root/'release').touch()
    elif trigger=='eof': os.write(master,b'\x04')
    deadline=time.monotonic()+8
    while p.poll() is None and time.monotonic()<deadline: drain(); time.sleep(.005)
    if p.poll() is None:
        p.send_signal(signal.SIGTERM); p.wait(timeout=3)
        raise AssertionError(f'{case}: did not terminate')
    drain(); rc=p.wait(); after=events(); log.write(tty); log.close()
    if master is not None: os.close(master)
    ok = (rc in (-signal.SIGINT,130) and after==before and (root/'marker').read_bytes()==marker) if expect_signal else (rc==0)
    if stage=='normal': ok &= after.count('provider')==2 and after.count('tool')==1 and after.count('write')==1 and 'continued' in after
    row=dict(case=case,rc=rc,actual_signal_sent=signal_sent,interrupt_requested=expect_signal,second_signal_sent=second_signal_sent,before=before,after=after,pass_=bool(ok))
    rows.append(row); print(json.dumps(row),flush=True)
for stage in ['before-provider','during-provider','after-response','before-write','after-write','before-retry','next-turn']:
    for trigger in ['pid','group','pty']: run(stage,trigger)
run('during-provider','repeat')
for trigger in ['pid','group','pty']: run('idle',trigger)
run('idle','pty',text=b'partial input')
run('idle','eof',expect_signal=False)
run('normal','none',expect_signal=False)
(out/'results.json').write_text(json.dumps(rows,indent=2))
assert all(r['pass_'] for r in rows), 'R4 signal contract failed; see results.json'
print(f'PASS {len(rows)} R4 cases')
PY
