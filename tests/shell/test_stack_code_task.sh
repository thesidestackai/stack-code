#!/usr/bin/env bash
# Offline, retained fixtures. Requires Linux bwrap, Python 3 and Git (no new
# production dependency). Host is read-only; only a fresh fixture root is writable.
# Optional first argument selects candidate bytes for the mutation campaign.
set -euo pipefail
TEST_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -I -B - "${1:-${TEST_DIR}/../../scripts/stack-code-task}" <<'PY'
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

TOOL = Path(sys.argv[1]).resolve()
ROOT = Path('/mnt/vast-data/git-worktrees')
R = Path(tempfile.mkdtemp(prefix='explicit-task-test-', dir=os.environ.get('TASK_TEST_TMPDIR', '/mnt/vast-data')))
candidate = R / 'candidate'; candidate.write_bytes(TOOL.read_bytes()); candidate.chmod(0o755)
TOOL = candidate
F = R / 'allowed'; F.mkdir()
CALLER = R / 'caller'; CALLER.mkdir()
ENV = {'HOME': str(R / 'home'), 'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8',
       'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': '/dev/null',
       'GIT_AUTHOR_NAME': 'Fixture', 'GIT_AUTHOR_EMAIL': 'fixture@example.invalid',
       'GIT_COMMITTER_NAME': 'Fixture', 'GIT_COMMITTER_EMAIL': 'fixture@example.invalid',
       'PYTHONDONTWRITEBYTECODE': '1', 'SENTINEL': 'spaces ; $(literal)'}
CANON = '/home/suki/.local/bin/stack-code'
DISPATCH = str(Path(CANON).resolve())
FAKE = R / 'launcher'
FAKE.write_text('''#!/usr/bin/python3
import json,os,sys,signal
if sys.argv[1:]==['--version']:
 if os.environ.get('VERSION_RECORD'):open(os.environ['VERSION_RECORD'],'w').write('version')
 print('stack-code daily-launcher-1');sys.exit(0)
with open(os.environ['RECORD'],'w') as f:
 json.dump({'cwd':os.getcwd(),'argv':sys.argv[1:],'sentinel':os.environ['SENTINEL']},f)
if os.environ.get('TERM_CHILD'):os.kill(os.getpid(),signal.SIGTERM)
sys.exit(int(os.environ.get('CHILD_EXIT','0')))
'''); FAKE.chmod(0o755)
BAD = R / 'wrong-launcher'; BAD.write_text('#!/bin/sh\necho unrelated-launcher\n'); BAD.chmod(0o755)
# Absolute Git double executes the real Git at another bind path. Only injected
# post-create observations differ; real worktree creation always happens.
REAL_GIT = R / 'real-git'; REAL_GIT.touch()
ENV['REAL_GIT'] = str(REAL_GIT)
GIT = R / 'git'
GIT.write_text('''#!/usr/bin/python3
import os,sys,subprocess,time,json
args=sys.argv[1:]; mode=os.environ.get('FAULT','');target=os.environ.get('TARGET','')
prefix=['--no-optional-locks','-C']; cmd=args[3:] if args[:2]==prefix else args
if cmd[:1]==['ls-remote'] and mode.startswith('remote-'):
 remote=cmd[3]
 with open(os.environ['PROBE_RECORD'],'a') as f:f.write(json.dumps({'remote':remote})+'\\n')
 if remote!='ahealthy':
  if mode=='remote-stall':time.sleep(.6);sys.exit(0)
  if mode in ('remote-auth','remote-transport'):
   print('SYNTHETIC_PRIVATE_REMOTE_DIAGNOSTIC',file=sys.stderr);sys.exit(128)
  if mode=='remote-stdin':
   data=sys.stdin.read()
   with open(os.environ['STDIN_RECORD'],'w') as f:f.write(repr(data))
   sys.exit(0 if data=='' else 128)
  if mode=='remote-prompt':sys.exit(0 if os.environ.get('GIT_TERMINAL_PROMPT')=='0' else 128)
if cmd[:2]==['worktree','add'] and mode=='add-failure':sys.exit(42)
if cmd[:2]==['worktree','add']:
 p=subprocess.run([os.environ['REAL_GIT'],*args])
 if p.returncode==0 and mode=='dirty':open(target+'/injected','w').write('dirty')
 if p.returncode==0 and mode=='tracked':open(target+'/sample.py','a').write('dirty')
 if p.returncode==0 and mode=='staged':
  open(target+'/sample.py','a').write('dirty')
  subprocess.run([os.environ['REAL_GIT'],'-C',target,'add','sample.py'],check=True)
 sys.exit(p.returncode)
if len(args)>2 and args[2]==target:
 if mode=='head' and cmd==['rev-parse','HEAD']:print('0'*40);sys.exit(0)
 if mode=='branch' and cmd==['symbolic-ref','--quiet','HEAD']:print('refs/heads/wrong');sys.exit(0)
os.execv(os.environ['REAL_GIT'],[os.environ['REAL_GIT'],*args])
'''); GIT.chmod(0o755)
# Scale only an already-present production bound. The real subprocess timeout
# still kills a sleeping fake Git; missing timeouts are never supplied by tests.
RUNNER = R / 'bounded-runner'
RUNNER.write_text('''#!/usr/bin/python3 -I
import runpy,subprocess,sys,os,json
real_run=subprocess.run
def run(args,**kw):
 if 'ls-remote' in args:
  bound=kw.get('timeout')
  with open(os.environ['BOUND_RECORD'],'a') as f:f.write(json.dumps({'timeout':bound})+'\\n')
  if bound is not None:
   assert bound==15,('unexpected production bound',bound)
   kw['timeout']=.15
 return real_run(args,**kw)
subprocess.run=run
sys.argv=sys.argv[1:]
runpy.run_path(sys.argv[0],run_name='__main__')
'''); RUNNER.chmod(0o755)
common = ['/usr/bin/bwrap','--ro-bind','/','/','--bind',str(R),str(R),
          '--bind',str(F),str(ROOT),'--unshare-net','--die-with-parent',
          '--dev','/dev','--proc','/proc','--chdir',str(CALLER)]
# Hide production registry behind an empty read-only fixture. Registry mutation
# attempts fail, while source assertions below catch even exception-swallowed access.
reg = R / 'registry'; reg.mkdir()
common += ['--ro-bind',str(reg),'/mnt/vast-data/stack-code-daily/workspace-registry']
count = 0
records = []

def git(repo,*args):
    return subprocess.check_output(['/usr/bin/git','-C',str(repo),*args],env=ENV,stderr=subprocess.PIPE).decode().strip()

def fixture():
    global count
    count += 1
    src = R / ('source-' + str(count));src.mkdir()
    git(src,'init','-q','-b','main')
    (src/'sample.py').write_text('value = 1\n')
    git(src,'add','sample.py');git(src,'commit','-qm','fixture')
    return src, git(src,'rev-parse','HEAD'), ROOT / ('target-' + str(count)), 'task-'+str(count)

def run(label, change=None, want=0, fault='', prepare=False, child=None, envmore=None, launcher=FAKE, missing=False, remote_reason=None):
    src,base,wt,branch=fixture()
    if change: src,base,wt,branch=change(src,base,wt,branch)
    rec=R/(label+'.json')
    before=(git(src,'show-ref'),git(src,'worktree','list','--porcelain'),git(src,'status','--porcelain')) if fault.startswith('remote-') else None
    args=['--repo',str(src),'--base',base,'--branch',branch,'--worktree',str(wt)]
    if prepare:args+=['--prepare-only']
    else:args+=['--',*(child if child is not None else ['--write','sample.py','task ; $(literal) "quotes"'])]
    binds=['--ro-bind','/usr/bin/git',str(REAL_GIT),'--ro-bind',str(GIT),'/usr/bin/git',
           '--ro-bind',str(launcher),DISPATCH]
    if missing:binds += ['--tmpfs','/home/suki/.local/bin']
    remote_test=fault.startswith('remote-')
    version=R/(label+'.version'); probes=R/(label+'.probes'); bounds=R/(label+'.bounds'); stdin=R/(label+'.stdin')
    entry=[str(RUNNER),str(TOOL)] if remote_test else [str(TOOL)]
    started=time.monotonic()
    p=subprocess.run(common+binds+['--',*entry,*args],env={**ENV,'RECORD':str(rec),'TARGET':str(wt),'FAULT':fault,
        'VERSION_RECORD':str(version),'PROBE_RECORD':str(probes),'BOUND_RECORD':str(bounds),'STDIN_RECORD':str(stdin),
        **(envmore or {})},input='OPERATOR_INPUT_SENTINEL\n',capture_output=True,text=True,timeout=30)
    elapsed=time.monotonic()-started
    if remote_test:
        observed=[json.loads(line)['remote'] for line in probes.read_text().splitlines()]
        assert observed==git(src,'remote').splitlines(),(label,observed)
        assert all(json.loads(line)['timeout']==15 for line in bounds.read_text().splitlines()),label
        assert 'SYNTHETIC_PRIVATE_REMOTE_DIAGNOSTIC' not in p.stderr,label
        if fault=='remote-stdin':assert stdin.read_text()==repr(''),label
        if remote_reason:
            assert remote_reason in p.stderr,(label,p.stderr)
            assert not version.exists() and not rec.exists() and not (F/wt.name).exists(),label
            assert not git(src,'for-each-ref','--format=%(refname)','refs/heads/'+branch),label
            assert before==(git(src,'show-ref'),git(src,'worktree','list','--porcelain'),git(src,'status','--porcelain')),label
            assert not list(reg.iterdir()),label
        if fault=='remote-stall':assert elapsed<3,(label,elapsed)

    (R/(label+'.stdout')).write_text(p.stdout);(R/(label+'.stderr')).write_text(p.stderr)
    assert p.returncode==want,(label,p.returncode,want,p.stderr)
    target=F/wt.name
    if want==0 or rec.exists():
        assert git(target,'rev-parse','HEAD')==base,label
        assert git(target,'symbolic-ref','--short','HEAD')==branch,label
        assert git(target,'status','--porcelain')=='',label
        assert (target/'sample.py').read_text()=='value = 1\n',label
    if rec.exists():
        data=json.loads(rec.read_text());assert data=={'cwd':str(wt),'argv':args[args.index('--')+1:],'sentinel':ENV['SENTINEL']},(label,data)
        assert not prepare,label
    elif want==0:assert prepare,label
    if fault in ('head','branch','dirty','tracked','staged'):
        assert target.is_dir() and git(src,'show-ref','--verify','refs/heads/'+branch),label
    if fault=='add-failure':
        assert not target.exists() and not rec.exists(),label
    if want==65 and fault!='child-exit':
        assert not rec.exists(),label
        if not fault:assert not target.exists(),label
    records.append({'case':label,'rc':p.returncode})
    print('PASS',label,flush=True)
    return src,wt,branch,p

for code in (0,1,65,130):
    # Exit 65 is a real child exit here, not a preflight refusal.
    if code==65:
        # run's refusal check is excluded via a benign nonempty fault label.
        run('exit-65',want=code,fault='child-exit',envmore={'CHILD_EXIT':str(code)})
    else:run('exit-'+str(code),want=code,envmore={'CHILD_EXIT':str(code)})
run('prepare',prepare=True)
run('signal-term',want=143,envmore={'TERM_CHILD':'1'})
run('invalid-base',change=lambda s,b,w,n:(s,'missing-revision',w,n),want=65)
run('leading-dash-base',change=lambda s,b,w,n:(s,'--help',w,n),want=65)
run('existing-checked-branch',change=lambda s,b,w,n:(s,b,w,'main'),want=65)
def existing(s,b,w,n):git(s,'branch',n);return s,b,w,n
run('existing-unchecked-branch',change=existing,want=65)
def target(s,b,w,n):(F/w.name).mkdir();return s,b,w,n
# For this refusal the target intentionally exists; check before invoking run.
# A separate exact refusal harness below checks preservation and error identity.
def refusal_case(label,setup,reason):
    s,b,w,n=fixture();s,b,w,n=setup(s,b,w,n)
    rec=R/(label+'.json')
    args=['--repo',str(s),'--base',b,'--branch',n,'--worktree',str(w)]
    p=subprocess.run(common+['--ro-bind',str(FAKE),DISPATCH,'--',str(TOOL),*args],env={**ENV,'RECORD':str(rec)},capture_output=True,text=True,timeout=30)
    (R/(label+'.stderr')).write_text(p.stderr)
    assert p.returncode==65 and reason in p.stderr and not rec.exists(),(label,p.returncode,p.stderr)
    records.append({'case':label,'rc':p.returncode});print('PASS',label,flush=True)
refusal_case('target-exists',target,'TARGET_EXISTS')
refusal_case('outside-root',lambda s,b,w,n:(s,b,R/'outside',n),'OUTSIDE_ALLOWED_ROOT')
refusal_case('prefix-collision',lambda s,b,w,n:(s,b,Path('/mnt/vast-data/git-worktrees2/new'),n),'OUTSIDE_ALLOWED_ROOT')
def symlink(s,b,w,n):
    (F/('link-'+n)).symlink_to(R,target_is_directory=True)
    return s,b,ROOT/('link-'+n)/'new',n
refusal_case('symlink-escape',symlink,'PARENT_NOT_PHYSICAL')
def staged(s,b,w,n):(s/'sample.py').write_text('staged\n');git(s,'add','sample.py');return s,b,w,n
run('staged-source',change=staged,want=65)
def dirty(s,b,w,n):(s/'sample.py').write_text('source dirty\n');(s/'untracked').write_text('keep');return s,b,w,n
src,_,_,_=run('dirty-source',change=dirty)
assert (src/'sample.py').read_text()=='source dirty\n' and (src/'untracked').read_text()=='keep'
run('git-add-failure',want=42,fault='add-failure')
for fault in ('head','branch','dirty','tracked','staged'):run('post-'+fault,want=65,fault=fault)
run('launcher-missing',want=65,missing=True)
run('launcher-incompatible',want=65,launcher=BAD)
run('argv-injection',child=['--write','space ; $(literal).py','--test','-leading value','task `literal` ; $(literal)'])
run('workspace-auto',want=65,child=['--workspace','auto'])
run('workspace-here',want=65,child=['--workspace','here'])
run('git-selector',want=65,envmore={'GIT_INDEX_FILE':str(R/'decoy')})
def nested(s,b,w,n):(s/'nested').mkdir();return s/'nested',b,w,n
run('nested-repository',change=nested,want=65)
run('relative-repository',change=lambda s,b,w,n:(Path('relative'),b,w,n),want=65)
# Distinct remote repository avoids conflating local and remote refusal.
def remote_branch(s,b,w,n):
    t=R/('remote-'+n);subprocess.run(['/usr/bin/git','clone','-q','--bare',str(s),str(t)],env=ENV,check=True)
    git(t,'update-ref','refs/heads/'+n,b);git(s,'remote','add','origin',str(t));return s,b,w,n
_,_,_,collision=run('remote-collision',change=remote_branch,want=65)
assert 'REMOTE_BRANCH_EXISTS' in collision.stderr
def remote_empty(s,b,w,n):git(s,'remote','add','origin',str(s));return s,b,w,n
run('remote-no-collision',change=remote_empty)
run('remote-timeout',change=remote_empty,want=65,fault='remote-stall',remote_reason='REMOTE_PROBE_TIMEOUT')
run('remote-stdin-eof',change=remote_empty,fault='remote-stdin')
run('remote-terminal-prompt-disabled',change=remote_empty,fault='remote-prompt',envmore={'GIT_TERMINAL_PROMPT':'1'})
for fault in ('auth','transport'):
    run('remote-'+fault,change=remote_empty,want=65,fault='remote-'+fault,remote_reason='REMOTE_PROBE_FAILED')
def multiple_remotes(s,b,w,n):
    git(s,'remote','add','ahealthy',str(s));git(s,'remote','add','zunknown',str(s))
    return s,b,w,n
run('remote-multiple-timeout',change=multiple_remotes,want=65,fault='remote-stall',remote_reason='REMOTE_PROBE_TIMEOUT')
run('remote-multiple-failure',change=multiple_remotes,want=65,fault='remote-auth',remote_reason='REMOTE_PROBE_FAILED')
run('remote-multiple-success',change=multiple_remotes,fault='remote-prompt')

def stale(s,b,w,n):
    # Build registry entry in the namespace, then retain its tree at another path.
    p=subprocess.run(common+['--','/usr/bin/git','-C',str(s),'worktree','add','-b','stale-'+n,str(w),b],env=ENV,capture_output=True)
    assert p.returncode==0,p.stderr
    (F/w.name).rename(F/(w.name+'-retained'))
    return s,b,w,n
refusal_case('registered-missing-target',stale,'WORKTREE_REGISTERED')
def checked_out_missing_ref(s,b,w,n):
    other=ROOT/('other-'+n)
    p=subprocess.run(common+['--','/usr/bin/git','-C',str(s),'worktree','add','-b',n,str(other),b],env=ENV,capture_output=True)
    assert p.returncode==0,p.stderr
    git(s,'update-ref','-d','refs/heads/'+n)
    return s,b,w,n
refusal_case('checked-out-missing-ref',checked_out_missing_ref,'BRANCH_CHECKED_OUT')
refusal_case('relative-target',lambda s,b,w,n:(s,b,Path('relative-target'),n),'WORKTREE_ABSOLUTE_NO_TRAVERSAL')
refusal_case('root-itself',lambda s,b,w,n:(s,b,ROOT,n),'OUTSIDE_ALLOWED_ROOT')
refusal_case('parent-traversal',lambda s,b,w,n:(s,b,ROOT/'..'/'escape',n),'WORKTREE_ABSOLUTE_NO_TRAVERSAL')
# Structural negative control supplements behavioral probes: explicit wrapper must
# never gain an automatic-registry/manager access, even in a swallowed-error branch.
source=TOOL.read_text()
assert 'workspace-registry' not in source and 'stack_code_workspace' not in source
assert not list(reg.iterdir())
(R/'results.json').write_text(json.dumps(records,indent=2))
print(f'{len(records)} passed; 0 failed; retained fixtures: {R}')
PY
