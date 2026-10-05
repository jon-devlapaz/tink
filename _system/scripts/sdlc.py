#!/usr/bin/env python3
"""Local workflow evidence. Human identity and release authority belong to the forge."""
import argparse
import contextlib
import errno
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
ITEM_ID = re.compile(r'[a-z0-9][a-z0-9-]{0,39}')
ITEM_KEYS = {'id', 'description', 'verify'}
CHECK_KEYS = {'argv', 'timeout_seconds'}
ARTIFACTS = ['01-plan/output/intent.md', '02-design/output/spec.md', '03-build/output/plan.md']


def digest(value):
    return hashlib.sha256(value).hexdigest()


def encoded(value):
    return json.dumps(value, sort_keys=True).encode()


def read_json(path):
    return json.loads(path.read_text())


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent)
    try:
        with os.fdopen(fd, 'w') as stream:
            json.dump(value, stream, indent=2)
            stream.write('\n')
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def run_path(slug):
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,79}', slug):
        raise ValueError('Run name must be 1–80 lowercase letters, digits, or hyphens; start with a letter or digit.')
    path = ROOT / 'runs' / slug
    if path.resolve() != path.absolute():
        raise ValueError('Run paths must not contain symlinks.')
    return path


def tink_lock_path(root=ROOT):
    repo_id = digest(str(root.resolve()).encode())[:12]
    return Path(tempfile.gettempdir()) / f'sdlc-tink-{os.getuid()}-{repo_id}.lock'


@contextlib.contextmanager
def locked(path, timeout=5.0, poll_interval=0.05):
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        fd = os.open(path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    except OSError as error:
        if error.errno == errno.ELOOP:
            raise ValueError(f'Lock path must not be a symlink: {path}') from error
        if error.errno == errno.EISDIR:
            raise ValueError(f'Stale directory lock found at {path}. Remove it to allow file flock.') from error
        raise
    st = os.fstat(fd)
    if not stat.S_ISREG(st.st_mode):
        os.close(fd)
        raise ValueError(f'Lock path must be a regular file: {path}')
    with open(fd, 'a+b', closefd=True) as handle:
        start = time.monotonic()
        while True:
            try:
                fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except OSError as error:
                if error.errno not in (errno.EAGAIN, errno.EACCES):
                    raise
                if time.monotonic() - start >= timeout:
                    raise ValueError(f'Busy or interrupted operation: {path}. Confirm no writer remains before removing this lock.')
                time.sleep(poll_interval)
        yield


def require_run(path):
    if not path.is_dir() or not (path / 'run.json').is_file():
        raise ValueError('Run is missing or legacy. Create a new run; legacy artifacts remain readable but are not approved evidence.')
    return read_json(path / 'run.json')


def validate_check(check, exact=False):
    """Shared argv/timeout rules for verification.json checks and checklist item checks."""
    if not isinstance(check, dict):
        raise ValueError('A check must be an object with argv and timeout_seconds.')
    if exact and set(check) != CHECK_KEYS:
        raise ValueError('A check must have exactly the keys argv, timeout_seconds.')
    argv = check.get('argv')
    if not isinstance(argv, list) or not argv or not all(isinstance(a, str) and a for a in argv):
        raise ValueError('Every check requires a nonempty argv array.')
    timeout = check.get('timeout_seconds')
    if not isinstance(timeout, int) or isinstance(timeout, bool) or timeout <= 0:
        raise ValueError('Every check requires a positive timeout_seconds.')


def validate_checklist(value):
    """Return the item list of a checklist definition, or raise ValueError naming the problem."""
    if not isinstance(value, dict) or set(value) != {'schema', 'items'}:
        raise ValueError('Checklist must be an object with exactly "schema" and "items".')
    if value['schema'] != 1 or isinstance(value['schema'], bool):
        raise ValueError('Checklist schema must be 1.')
    items = value['items']
    if not isinstance(items, list):
        raise ValueError('Checklist items must be a list.')
    seen = set()
    for index, item in enumerate(items):
        name = item.get('id') if isinstance(item, dict) and isinstance(item.get('id'), str) else f'#{index + 1}'
        if not isinstance(item, dict) or not ITEM_KEYS <= set(item) <= ITEM_KEYS | {'check'}:
            raise ValueError(f'Checklist item {name} must have the keys id, description, verify, and optionally check.')
        if not ITEM_ID.fullmatch(item['id']):
            raise ValueError(f'Checklist item {name} has an invalid id (use [a-z0-9][a-z0-9-]{{0,39}}).')
        if item['id'] in seen:
            raise ValueError(f'Checklist item {name} has a duplicate id.')
        seen.add(item['id'])
        for key in ('description', 'verify'):
            if not isinstance(item[key], str) or not item[key].strip():
                raise ValueError(f'Checklist item {name} needs a nonempty {key}.')
        if 'check' in item:
            try:
                validate_check(item['check'], exact=True)
            except ValueError as error:
                raise ValueError(f'Checklist item {name} has an invalid check: {error}') from error
    return items


def load_checklist(path):
    """None for legacy runs without checklist.json; otherwise the validated item list."""
    target = path / 'checklist.json'
    if not target.exists():
        return None
    try:
        return validate_checklist(read_json(target))
    except json.JSONDecodeError as error:
        raise ValueError(f'checklist.json is not valid JSON: {error}') from error


def checklist_digest(path):
    """Digest of canonical definitions (whitespace/key order insensitive); None when absent."""
    target = path / 'checklist.json'
    if not target.exists():
        return None
    try:
        return digest(encoded(read_json(target)))
    except ValueError:
        return digest(target.read_bytes())


def marks(path, items):
    """Latest valid receipt per current item id; receipts for removed items are ignored."""
    ids = {item['id'] for item in items}
    latest_marks = {}
    for receipt in (path / 'marks').glob('*.json') if (path / 'marks').is_dir() else []:
        try:
            record = read_json(receipt)
            key = (record['time_ns'], receipt.name)
            if record['item'] in ids and record['result'] in ('passed', 'failed') and isinstance(record['time_ns'], int):
                if record['item'] not in latest_marks or key > latest_marks[record['item']][0]:
                    latest_marks[record['item']] = (key, record)
        except (ValueError, KeyError, TypeError, OSError):
            continue
    return {item: record for item, (_, record) in latest_marks.items()}


def inputs(path, stage):
    metadata = require_run(path)
    files = [path / 'brief.md'] if metadata['profile'] == 'light' else [path / p for p in ARTIFACTS[:stage]]
    files += sorted((ROOT / 'stages').glob('*/CONTEXT.md'))
    values = {str(p.relative_to(ROOT)): digest(p.read_bytes()) for p in files}
    seed_contract = metadata.get('seed_contract')
    if seed_contract:
        source = Path(seed_contract['path'])
        source = source if source.is_absolute() else ROOT / source
        values['seed-contract'] = digest(source.read_bytes()) if source.is_file() else '<missing>'
    if stage == 3 and checklist_digest(path) is not None:
        values[str((path / 'checklist.json').relative_to(ROOT))] = checklist_digest(path)
    return digest(encoded(values))


def latest(path, stage):
    records = sorted((path / 'decisions').glob(f'{stage}-*.json'))
    return read_json(records[-1]) if records else None


def gate(path, stage):
    record = latest(path, stage)
    if record is None:
        return 'pending'
    if record['inputs'] != inputs(path, stage):
        return 'stale'
    return record['decision']


def stages(path):
    return [3] if require_run(path)['profile'] == 'light' else [1, 2, 3]


def ready(path):
    for stage in stages(path):
        if gate(path, stage) != 'approved':
            raise ValueError(f'Stage {stage} needs a current approval receipt.')


def git(*args):
    return subprocess.check_output(['git', '-C', str(ROOT), *args], stderr=subprocess.PIPE)


def strip_generated_rules(data):
    """Drop each well-formed tink:rules block (so a removed block equals an excluded one); unbalanced markers are left as-is."""
    lines = data.decode('utf-8', errors='surrogateescape').splitlines(keepends=True)
    blocks, problems = rules_blocks(lines)
    if not blocks or any('duplicate' not in problem for problem in problems):
        return data
    out, cursor = [], 0
    for begin, end, _ in blocks:
        out.append(''.join(lines[cursor:begin]).encode('utf-8', errors='surrogateescape'))
        cursor = end + 1
    out.append(''.join(lines[cursor:]).encode('utf-8', errors='surrogateescape'))
    return b''.join(out)


def disposable(relative):
    """Untracked paths that are generated, not authored: Python caches and what tink mounts leave in .tink/.

    Tracked files stay covered even when their name resembles generated output."""
    path = Path(relative)
    if any(part in {'__pycache__', '.pytest_cache', '.ruff_cache'} for part in path.parts) or path.suffix in {'.pyc', '.pyo'}:
        return True
    return relative == '.tink/.gitignore' or relative.startswith(('.tink/.active/', '.tink/cache/'))


def snapshot_files():
    """Per-file fingerprint map of the candidate: {relative: [digest, mode]}."""
    tracked = set(git('ls-files', '-z', '--cached').split(b'\0'))
    names = tracked | set(git('ls-files', '-z', '--others', '--exclude-standard').split(b'\0'))
    files = {}
    for name in names:
        if not name:
            continue
        relative = os.fsdecode(name)
        if relative.startswith('runs/'):
            continue
        if name not in tracked and disposable(relative):
            continue
        path = ROOT / relative
        if path.is_symlink():
            value = os.readlink(path).encode()
        elif path.is_file():
            value = path.read_bytes()
            if relative == 'AGENTS.md':
                value = strip_generated_rules(value)
        elif not path.exists():
            value = b'<deleted>'
        else:
            raise ValueError(f'Unsupported candidate path: {relative}')
        files[relative] = [digest(value), path.lstat().st_mode if path.exists() or path.is_symlink() else 0]
    return files


def snapshot():
    head = git('rev-parse', 'HEAD').decode().strip()
    return {'head': head, 'tree': digest(encoded(snapshot_files()))}


def tracked_paths():
    return {os.fsdecode(n) for n in git('ls-files', '-z', '--cached').split(b'\0') if n}


def changed_report(before_files, after_files):
    changed = sorted(name for name in set(before_files) | set(after_files) if before_files.get(name) != after_files.get(name))
    if not changed:
        return ''
    shown = changed[:5]
    text = ' changed: ' + ', '.join(shown) + (f' (+{len(changed) - 5} more)' if len(changed) > 5 else '')
    tracked = tracked_paths()
    if any(name in tracked and (Path(name).suffix in {'.pyc', '.pyo'} or '__pycache__' in Path(name).parts) for name in shown):
        text += ' Hint: tracked bytecode files change during test runs; untrack them and ignore __pycache__'
    return text


def checks_config():
    config = read_json(ROOT / '_system/verification.json')
    checks = config.get('checks')
    if not isinstance(config.get('require_tink'), bool) or not isinstance(checks, list) or not checks:
        raise ValueError('Verification requires an explicit Tink policy and a nonempty check list.')
    for check in checks:
        validate_check(check)
    return config


def test_lock(path):
    lock = path / 'test-lock.json'
    if not lock.exists():
        if require_run(path)['kind'] == 'bug':
            raise ValueError('Bug runs require a reproduction test lock.')
        return None
    record = read_json(lock)
    for name, expected in record['files'].items():
        target = ROOT / name
        if target.is_symlink() or not target.is_file() or digest(target.read_bytes()) != expected:
            raise ValueError(f'Locked test input changed or disappeared: {name}')
    return digest(lock.read_bytes())


def manual_marks(path):
    """Digest of the latest receipt per attested (check-less) item: a later mark, or an edited receipt, stales verification."""
    items = load_checklist(path) or []
    latest = marks(path, items)
    return {item['id']: digest(encoded(latest[item['id']])) for item in items if 'check' not in item and item['id'] in latest}


def evidence_inputs(path):
    attested = manual_marks(path)
    return {'candidate': snapshot(), 'inputs': inputs(path, 3),
            'policy': digest((ROOT / '_system/verification.json').read_bytes()),
            'test_lock': test_lock(path),
            **({'checklist': checklist_digest(path), 'manual_marks': attested} if checklist_digest(path) is not None else {})}


def create(args):
    path = run_path(args.run)
    path.parent.mkdir(exist_ok=True)
    with locked(path.parent / f'.{args.run}.lock'):
        if path.exists():
            raise ValueError('Run already exists; nothing overwritten.')
        temporary = Path(tempfile.mkdtemp(prefix=f'.{args.run}-', dir=path.parent))
        try:
            write_json(temporary / 'run.json', {'profile': args.profile, 'kind': args.kind, 'schema': 1})
            write_json(temporary / 'checklist.json', {'schema': 1, 'items': []})
            if args.profile == 'light':
                shutil.copyfile(ROOT / '_shared/brief-template.md', temporary / 'brief.md')
            else:
                for target, template in zip(ARTIFACTS, ['intent', 'spec', 'plan']):
                    dest = temporary / target
                    dest.parent.mkdir(parents=True)
                    shutil.copyfile(ROOT / '_shared' / f'{template}-template.md', dest)
            temporary.rename(path)
        finally:
            if temporary.exists():
                shutil.rmtree(temporary)
    print(f'Created {path}. Next: define the change and obtain a human review.')


def decide(args):
    path = run_path(args.run)
    require_run(path)
    with locked(path / '.writer-lock'):
        if args.stage not in stages(path):
            raise ValueError('Light runs use stage 3 for their combined definition gate.')
        if args.decision == 'approved':
            bound = require_run(path).get('seed_contract')
            if bound:
                source = Path(bound['path'])
                source = source if source.is_absolute() else ROOT / source
                if not source.is_file() or digest(source.read_bytes()) != bound['sha256']:
                    raise ValueError(f"The seed contract {bound['path']} changed or is missing since stage 1 was opened; "
                                     f'confirm it again, then run: sdlc.py stage {args.run} 1 --seed-contract {bound.get("source", bound["path"])}')
        if args.decision == 'approved' and args.stage == 3 and load_checklist(path) == []:
            raise ValueError(f'Define at least one checklist item before approval. Edit runs/{args.run}/checklist.json '
                             '(each item needs id, description, verify; optional check).')
        if args.decision == 'approved':
            for stage in stages(path):
                if stage < args.stage and gate(path, stage) != 'approved':
                    raise ValueError(f'Approve stage {stage} first.')
        write_json(path / 'decisions' / f'{args.stage}-{time.time_ns()}-{uuid.uuid4().hex}.json', {
            'inputs': inputs(path, args.stage), 'decision': args.decision,
            **({'has_checklist': True} if args.stage == 3 and (path / 'checklist.json').exists() else {}),
            'reviewer': args.reviewer, 'source': args.source, 'reason': args.reason})
    print('Recorded local review receipt. Release approval must be independently authenticated by the forge.')


def describe(error):
    """Error text plus any pointer to the check output attached where the check ran."""
    return f"{error}{getattr(error, 'sdlc_hint', '')}"


def verify(args):
    path = run_path(args.run)
    require_run(path)
    with locked(path / '.writer-lock'):
        output = path / '04-test/output'
        log_hint = f' (output: {(output / "test-log.md").relative_to(ROOT)})'
        output.mkdir(parents=True, exist_ok=True)
        receipt = output / 'verification.json'
        write_json(receipt, {'result': 'running'})
        try:
            ready(path)
            config = checks_config()
            before_files = snapshot_files()
            before = evidence_inputs(path)
            items = load_checklist(path)
            checks = list(config['checks'])
            if config['require_tink']:
                checks.insert(0, {'argv': ['tink', 'skill', 'check'], 'timeout_seconds': 120})
            with (output / 'test-log.md').open('w') as log:
                for check in checks:
                    log.write(f"\n$ {json.dumps(check['argv'])}\n")
                    log.flush()
                    try:
                        result = subprocess.run(check['argv'], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
                                                timeout=check['timeout_seconds'])
                    except (subprocess.TimeoutExpired, OSError) as error:
                        log.write(f'{type(error).__name__}: {error}\n')
                        error.sdlc_hint = log_hint
                        raise
                    if result.returncode:
                        raise ValueError(f"Check failed ({result.returncode}): {check['argv']}{log_hint}")
                for item in items or []:
                    if 'check' not in item:
                        continue
                    check = item['check']
                    log.write(f"\n# checklist item {item['id']}\n$ {json.dumps(check['argv'])}\n")
                    log.flush()
                    try:
                        result = subprocess.run(check['argv'], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
                                                timeout=check['timeout_seconds'])
                    except (subprocess.TimeoutExpired, OSError) as error:
                        log.write(f'{type(error).__name__}: {error}\n')
                        raise ValueError(f"Checklist check failed: {item['id']}{log_hint}") from error
                    if result.returncode:
                        raise ValueError(f"Checklist check failed: {item['id']}{log_hint}")
            if before != evidence_inputs(path):
                raise ValueError('Candidate or inputs changed during verification; rerun against stable inputs.'
                                 + changed_report(before_files, snapshot_files()))
            ready(path)
            proof = {}
            if items is not None:
                marked = marks(path, items)
                attested = [item['id'] for item in items if 'check' not in item]
                missing = [i for i in attested if marked.get(i, {}).get('result') != 'passed']
                if missing:
                    raise ValueError(f"Checklist incomplete: {', '.join(missing)} "
                                     f'(mark them with: sdlc.py mark {args.run} <id> passed --evidence "...")')
                proof = {'checklist_items': {item['id']: 'proven' if 'check' in item else 'attested' for item in items}}
            write_json(receipt, {'result': 'passed', **before, **proof, 'log': digest((output / 'test-log.md').read_bytes())})
        except Exception as error:
            write_json(receipt, {'result': 'failed', 'error': describe(error)})
            raise
    print('Configured checks passed for the recorded candidate. Human release review remains required.')


def lock_tests(args):
    path = run_path(args.run)
    require_run(path)
    with locked(path / '.writer-lock'):
        record = path / 'test-lock.json'
        if record.exists():
            raise ValueError('Test lock already exists. A corrected baseline requires a new run and independent review.')
        files = {}
        for name in args.paths:
            target = ROOT / name
            if target.resolve() != target.absolute() or not target.is_file() or not target.is_relative_to(ROOT) or '..' in Path(name).parts:
                raise ValueError('Lock only regular files inside the checkout.')
            files[str(target.relative_to(ROOT))] = digest(target.read_bytes())
        write_json(record, {'files': files, 'candidate': snapshot(), 'review_source': args.source,
                            'failure_evidence': args.failure_evidence})
    print('Local test baseline recorded. CI must independently validate the failing reproduction and protect the baseline.')


def mark(args):
    path = run_path(args.run)
    require_run(path)
    evidence = args.evidence.strip()
    if not evidence:
        raise ValueError('Evidence must be nonempty.')
    with locked(path / '.writer-lock'):
        items = load_checklist(path)
        if items is None:
            raise ValueError('Run has no checklist.json (legacy run); nothing to mark.')
        if args.item not in {item['id'] for item in items}:
            ids = ', '.join(item['id'] for item in items)
            raise ValueError(f'Unknown checklist item: {args.item} (items: {ids})')
        if any(item['id'] == args.item and 'check' in item for item in items):
            raise ValueError(f'item {args.item} has an executable check; run verify to prove it '
                             f'(its check runs during: python3 _system/scripts/sdlc.py verify {args.run})')
        if gate(path, 3) != 'approved':
            raise ValueError('Stage 3 needs a current approval receipt before items can be marked.')
        now = time.time_ns()
        write_json(path / 'marks' / f'{args.item}-{now}-{uuid.uuid4().hex}.json', {
            'item': args.item, 'result': args.result, 'evidence': evidence,
            'candidate': snapshot()['tree'], 'time_ns': now})
    print(f'Recorded {args.result} receipt for {args.item}.')


def print_checklist(path, verified=False):
    try:
        items = load_checklist(path)
    except ValueError as error:
        print(f'Checklist: invalid ({error})')
        return
    if items is None:
        decision = latest(path, 3)
        print('Checklist: MISSING (deleted after approval)' if decision and decision.get('has_checklist')
              else 'Checklist: none (legacy run)')
        return
    if not items:
        print('Checklist: no items defined')
        return
    try:
        tree = snapshot()['tree']
    except (ValueError, OSError, subprocess.SubprocessError):
        tree = None
    marked = marks(path, items)
    lines = []
    passed = 0
    for item in items:
        record = marked.get(item['id'])
        if 'check' in item:
            if verified:
                passed += 1
            else:
                lines.append(f"  - {item['id']}: pending (proved by verify)")
        elif record is None:
            lines.append(f"  - {item['id']}: pending")
        elif record['result'] != 'passed':
            lines.append(f"  - {item['id']}: failed")
        else:
            passed += 1
            if tree is not None and record.get('candidate') != tree:
                lines.append(f"  - {item['id']}: attested before the latest changes; re-check it only if the change affects it")
    proven = sum('check' in item for item in items)
    summary = f'Checklist: {passed}/{len(items)} passed ({proven} proven by check, {len(items) - proven} attested)'
    print('\n'.join([summary] + lines))


def evidence_matches(record, current):
    # HEAD records provenance. Evidence-only commits must not stale unchanged
    # candidate contents; CI still needs a check for the actual merge revision.
    return (record.get('candidate', {}).get('tree') == current['candidate']['tree']
            and all(record.get(key) == value for key, value in current.items() if key != 'candidate'))


def status(args):
    if not args.run:
        directory = ROOT / 'runs'
        print('\n'.join(p.name for p in sorted(directory.iterdir()) if p.is_dir() and not p.name.startswith('.')) if directory.exists() else 'No runs.')
        return
    path = run_path(args.run)
    require_run(path)
    blocked = False
    for stage in stages(path):
        state = gate(path, stage)
        print(f'Stage {stage}: {state}' + (' (blocked by upstream gate)' if blocked else ''))
        if state != 'approved' and not blocked:
            print(f'Next: revise/review stage {stage}; record the human decision.')
            print('After human review, fill in this command from the scaffold root '
                  '(DECISION: approved or changes-requested):')
            print(f"  python3 _system/scripts/sdlc.py decide {args.run} {stage} DECISION "
                  "--reviewer 'REVIEWER' --source 'SOURCE' --reason 'REASON'")
            blocked = True
    record_path = path / '04-test/output/verification.json'
    record = read_json(record_path) if record_path.exists() else None
    verified = False
    if record is not None and record.get('result') == 'passed':
        try:
            verified = evidence_matches(record, evidence_inputs(path)) and record['log'] == digest((record_path.parent / 'test-log.md').read_bytes())
        except (ValueError, OSError, subprocess.SubprocessError, KeyError):
            pass
    print_checklist(path, verified)
    if record is not None:
        print('Verification: ' + ('current' if verified and not blocked else 'failed, stale, or blocked'))
    else:
        print('Verification: not run (implementation may be pending; a text log is not passing evidence)')
    if not blocked:
        print('Next: independent PR review and release checks.' if verified else 'Next: implement the approved brief, then run verification.')
    print('Deployment: not inferred from local review files; consult the deployment system.')


TOOL_ACCEPTABLE_CODES = {
    'tink': (0,),
}


def skills(args):
    allowed = TOOL_ACCEPTABLE_CODES.get(args.tool)
    if allowed is None:
        raise ValueError(f'Unknown skill tool: {args.tool!r}')
    # All cooperating worktrees on this host share a tink mutation lock. This is not
    # a security boundary and cannot coordinate tools invoked outside this wrapper.
    for relative in ['.agents', '.agents/skills', '.tink']:
        target = ROOT / relative
        if target.resolve() != target.absolute():
            raise ValueError('Skill state must not be symlinked across worktrees.')
    arguments = list(args.arguments)
    if arguments[:1] == ['--']:
        arguments.pop(0)
    if not arguments:
        raise ValueError('Supply the authorized Tink operation.')
    lock = tink_lock_path(ROOT)
    with locked(lock):
        result = subprocess.run([args.tool, *arguments], cwd=ROOT)
        if result.returncode not in allowed:
            raise ValueError(f'{args.tool} failed with exit code {result.returncode}; inspect partial state before retrying.')


# --- walk lint -------------------------------------------------------------
# Structural preconditions of the ICM "walk test". It checks structure only; it
# cannot prove that an agent with no memory can actually orient from the files.
ROUTER_BEGIN = '<!-- AI-Native SDLC Router -->'
ROUTER_END = '<!-- End AI-Native SDLC Router -->'
RULES_BEGIN = re.compile(r'<!--\s*tink:rules begin\b(.*?)-->')
RULES_END = re.compile(r'<!--\s*tink:rules end\s*-->')
ENTRY_LINE_LIMIT = 60
GENERATED_BYTE_LIMIT = 8192
TOKEN_WARN, TOKEN_FAIL = 8000, 16000
POINTER_PREFIXES = ('_system/', '_shared/', 'stages/', 'runs/', '.tink/', '.agents/')
SKILLSET_TOKEN = re.compile(r'(?<![A-Za-z0-9_-])[a-z][a-z0-9]*-skillset')
WALK_TIMEOUT = 30


def walk_result(cid, name, status, title, detail='', items=None):
    return {'id': cid, 'name': name, 'status': status, 'title': title, 'detail': detail, 'items': items or []}


def read_text_safe(path):
    try:
        return path.read_text(errors='replace')
    except OSError:
        return None


def rules_blocks(lines):
    """Return (blocks, problems). blocks: (begin_idx, end_idx, begin_line) for closed blocks."""
    blocks, problems, open_at = [], [], None
    for number, line in enumerate(lines):
        if RULES_BEGIN.search(line):
            if open_at is not None:
                problems.append(f'AGENTS.md:{number + 1}: tink:rules begin inside an unclosed block (began line {open_at + 1})')
            open_at = number
        elif RULES_END.search(line):
            if open_at is None:
                problems.append(f'AGENTS.md:{number + 1}: tink:rules end without begin')
            else:
                blocks.append((open_at, number, lines[open_at]))
                open_at = None
    if open_at is not None:
        problems.append(f'AGENTS.md:{open_at + 1}: tink:rules begin without end')
    if len(blocks) > 1:
        problems.append('AGENTS.md: duplicate tink:rules blocks at lines ' + ', '.join(str(b[0] + 1) for b in blocks))
    return blocks, problems


def walk_stage_dirs(root):
    stages_dir = root / 'stages'
    return sorted(p for p in stages_dir.iterdir() if p.is_dir() and (p / 'CONTEXT.md').is_file()) if stages_dir.is_dir() else []


def walk_w1(root, agents_text):
    name = 'entry-file'
    if agents_text is None:
        return walk_result('W1', name, 'fail', 'AGENTS.md is missing', 'The entry file must exist at the repo root.', ['AGENTS.md'])
    lines = agents_text.splitlines()
    problems = []
    begin = next((i for i, l in enumerate(lines) if ROUTER_BEGIN in l), None)
    end = next((i for i, l in enumerate(lines) if ROUTER_END in l), None)
    if begin is None or end is None or end < begin:
        problems.append(f'AGENTS.md: SDLC router markers missing or misordered: {ROUTER_BEGIN} ... {ROUTER_END}')
    blocks, block_problems = rules_blocks(lines)
    problems += block_problems
    generated = set()
    sizes = []
    for start, stop, _ in blocks:
        generated.update(range(start, stop + 1))
        size = sum(len(l.encode()) + 1 for l in lines[start + 1:stop])
        sizes.append(size)
        if size > GENERATED_BYTE_LIMIT:
            problems.append(f'AGENTS.md:{start + 1}: generated block is {size} bytes (limit {GENERATED_BYTE_LIMIT})')
    outside = len(lines) - len(generated)
    if outside > ENTRY_LINE_LIMIT:
        problems.append(f'AGENTS.md: {outside} lines outside generated blocks (limit {ENTRY_LINE_LIMIT})')
    info = f'{outside} lines outside generated blocks; ' + (f'generated block {sizes[0]} bytes (cap {GENERATED_BYTE_LIMIT})' if sizes else 'no generated block')
    if problems:
        return walk_result('W1', name, 'fail', 'entry file breaks the router contract', info, problems)
    return walk_result('W1', name, 'pass', f'AGENTS.md is a router ({info})', info)


def pointer_candidates(text, first_line=1):
    for offset, line in enumerate(text.splitlines()):
        for span in re.findall(r'`([^`\n]+)`', line):
            token = span.split()[0].rstrip('.,:;)')
            if token.startswith(POINTER_PREFIXES):
                yield first_line + offset, token


def pointer_resolves(root, token):
    if token.startswith('runs/'):
        return True
    if '<' not in token:
        return (root / token).exists()
    prefix = token.split('<', 1)[0]
    prefix_dir = prefix.rsplit('/', 1)[0] if '/' in prefix else ''
    if not (root / prefix_dir).is_dir():
        return False
    pattern = re.sub(r'<[^>]*>', '*', token)
    return any(root.glob(pattern))


def walk_w2(root, agents_text):
    name = 'pointers-resolve'
    sources = []
    if agents_text is not None:
        lines = agents_text.splitlines()
        begin = next((i for i, l in enumerate(lines) if ROUTER_BEGIN in l), None)
        end = next((i for i, l in enumerate(lines) if ROUTER_END in l), None)
        if begin is not None and end is not None and end > begin:
            sources.append(('AGENTS.md', '\n'.join(lines[begin:end + 1]), begin + 1))
    for stage in walk_stage_dirs(root):
        text = read_text_safe(stage / 'CONTEXT.md')
        if text is not None:
            sources.append((f'stages/{stage.name}/CONTEXT.md', text, 1))
    bad, count = [], 0
    for label, text, first in sources:
        for line, token in pointer_candidates(text, first):
            count += 1
            if not pointer_resolves(root, token):
                bad.append(f'{label}:{line}: `{token}` does not resolve')
    if bad:
        return walk_result('W2', name, 'fail', f'{len(bad)} dead pointer(s)', 'Backticked repo paths must exist.', bad)
    return walk_result('W2', name, 'pass', f'{count} pointers resolve', '')


def walk_w3(root):
    name = 'stage-contracts'
    stages_found = walk_stage_dirs(root)
    if not stages_found:
        return walk_result('W3', name, 'fail', 'no stage contracts found', 'Expected stages/<stage-name>/CONTEXT.md.', ['stages/'])
    missing = []
    for stage in stages_found:
        text = read_text_safe(stage / 'CONTEXT.md') or ''
        checks = [('Inputs', r'^\s*inputs?\b\s*:'), ('Output', r'^\s*outputs?\b\s*:'), ('Gate', r'\bgate\s*:|\bhuman\b')]
        for label, pattern in checks:
            if not re.search(pattern, text, re.I | re.M):
                missing.append(f'stages/{stage.name}/CONTEXT.md: missing {label} statement')
    if missing:
        return walk_result('W3', name, 'fail', f'{len(missing)} contract statement(s) missing',
                           'Each stage names Inputs:, Output:, and a human Gate:.', missing)
    return walk_result('W3', name, 'pass', f'{len(stages_found)} stage contracts name inputs, output, and gate', '')


def walk_w4(root, agents_text):
    name = 'token-budget'
    agents_chars = len(agents_text) if agents_text is not None else 0
    tokens = {}
    for stage in walk_stage_dirs(root):
        text = read_text_safe(stage / 'CONTEXT.md') or ''
        total = agents_chars + len(text)
        for shared in sorted(set(re.findall(r'_shared/[A-Za-z0-9_.-]+', text))):
            shared_text = read_text_safe(root / shared) if (root / shared).is_file() else None
            if shared_text is not None:
                total += len(shared_text)
        tokens[stage.name] = total // 4
    over_fail = [f'{s}: ~{t} tokens (limit {TOKEN_FAIL})' for s, t in tokens.items() if t > TOKEN_FAIL]
    over_warn = [f'{s}: ~{t} tokens (warn above {TOKEN_WARN})' for s, t in tokens.items() if TOKEN_WARN < t <= TOKEN_FAIL]
    detail = 'Estimate: (AGENTS.md + CONTEXT.md + referenced _shared files) chars / 4.'
    if over_fail:
        return walk_result('W4', name, 'fail', f'{len(over_fail)} stage(s) over {TOKEN_FAIL} tokens', detail, over_fail + over_warn), tokens
    if over_warn:
        return walk_result('W4', name, 'warn', f'{len(over_warn)} stage(s) over {TOKEN_WARN} tokens', detail, over_warn), tokens
    peak = max(tokens.values(), default=0)
    return walk_result('W4', name, 'pass', f'largest stage context ~{peak} tokens', detail), tokens


def walk_w5(root):
    name = 'skillsets-consistent'
    pins_dir = root / '.tink/skillsets'
    if not pins_dir.is_dir():
        return walk_result('W5', name, 'warn', 'no stage skillset pins installed', 'Expected pins in .tink/skillsets/.')
    pins = {p.stem for p in pins_dir.glob('*.json')}
    sources = [(f'stages/{s.name}/CONTEXT.md', s / 'CONTEXT.md') for s in walk_stage_dirs(root)]
    sources.append(('_system/SDLC.md', root / '_system/SDLC.md'))
    missing, named = [], set()
    for label, path in sources:
        text = read_text_safe(path)
        if text is None:
            continue
        for number, line in enumerate(text.splitlines(), 1):
            for token in SKILLSET_TOKEN.findall(line):
                if label.startswith('stages/'):
                    named.add(token)
                if token not in pins:
                    missing.append(f'{label}:{number}: {token} has no .tink/skillsets/{token}.json')
    if missing:
        return walk_result('W5', name, 'fail', f'{len(missing)} skillset name(s) without a pin', 'Every named skillset needs a committed pin.', missing)
    unreferenced = sorted(pins - named)
    if unreferenced:
        return walk_result('W5', name, 'warn', f'{len(unreferenced)} pin(s) no stage names', 'Pins should be named by a stage CONTEXT.md.',
                           [f'.tink/skillsets/{p}.json' for p in unreferenced])
    return walk_result('W5', name, 'pass', f'{len(pins)} pins, all named and present', '')


def tail_lines(text, count=3):
    return [l for l in text.splitlines() if l.strip()][-count:]


def walk_w6(root):
    name = 'status-derivable'
    script = root / '_system/scripts/sdlc.py'
    runs_dir = root / 'runs'
    slugs = sorted(p.name for p in runs_dir.iterdir() if p.is_dir() and not p.name.startswith('.') and (p / 'run.json').is_file()) if runs_dir.is_dir() else []
    if not slugs:
        return walk_result('W6', name, 'pass', 'sdlc.py is present', 'no runs')
    bad = []
    for slug in slugs:
        try:
            result = subprocess.run([sys.executable, str(script), 'status', slug], cwd=root, capture_output=True, text=True, timeout=WALK_TIMEOUT)
        except subprocess.TimeoutExpired:
            bad.append(f'runs/{slug}: status timed out after {WALK_TIMEOUT}s')
            continue
        except OSError as error:
            bad.append(f'runs/{slug}: cannot run status: {error}')
            continue
        if result.returncode != 0:
            bad.append(f'runs/{slug}: status exit {result.returncode}: ' + ' | '.join(tail_lines(result.stderr + '\n' + result.stdout)))
    if bad:
        return walk_result('W6', name, 'fail', f'{len(bad)} run(s) whose status cannot be derived', 'python3 _system/scripts/sdlc.py status <slug> must exit 0.', bad)
    return walk_result('W6', name, 'pass', f'status derivable for {len(slugs)} run(s)', '')


def walk_w7(root, agents_text):
    name = 'rules-block-current'
    blocks, problems = rules_blocks(agents_text.splitlines()) if agents_text is not None else ([], [])
    if not blocks or problems:
        return walk_result('W7', name, 'pass', 'skipped (no well-formed tink:rules block)', '')
    tink = shutil.which('tink')
    if tink is None:
        return walk_result('W7', name, 'pass', 'skipped (tink not on PATH)', '')
    match = re.search(r'skillset=(\S+)', RULES_BEGIN.search(blocks[0][2]).group(1))
    if not match:
        return walk_result('W7', name, 'warn', 'rules block names no skillset', 'Expected skillset=<name> in the begin marker.', [f'AGENTS.md:{blocks[0][0] + 1}'])
    skillset = match.group(1)
    try:
        result = subprocess.run([tink, 'use', skillset, '--check'], cwd=root, capture_output=True, text=True, timeout=WALK_TIMEOUT)
    except (subprocess.TimeoutExpired, OSError) as error:
        return walk_result('W7', name, 'warn', 'tink use --check did not complete', str(error))
    message = ' | '.join(tail_lines(result.stdout + '\n' + result.stderr)) or f'exit {result.returncode}'
    if result.returncode == 0:
        return walk_result('W7', name, 'pass', f'rules block current for {skillset}', '')
    if result.returncode == 1:
        return walk_result('W7', name, 'fail', f'rules block is stale for {skillset}', message, [f'AGENTS.md:{blocks[0][0] + 1}'])
    return walk_result('W7', name, 'warn', f'tink use --check exited {result.returncode}', message)


def walk(args):
    agents_text = read_text_safe(ROOT / 'AGENTS.md')
    w4, tokens = walk_w4(ROOT, agents_text)
    checks = [walk_w1(ROOT, agents_text), walk_w2(ROOT, agents_text), walk_w3(ROOT), w4,
              walk_w5(ROOT), walk_w6(ROOT), walk_w7(ROOT, agents_text)]
    summary = {key: sum(1 for c in checks if c['status'] == key) for key in ('pass', 'warn', 'fail')}
    if args.json:
        print(json.dumps({'schema': 1, 'checks': checks, 'tokens': tokens, 'summary': summary}, indent=2))
    else:
        for c in checks:
            print(f"{c['status'].upper()} {c['id']} {c['name']}: {c['title']}")
            if c['status'] != 'pass':
                if c['detail']:
                    print(f"  {c['detail']}")
                for item in c['items']:
                    print(f'  - {item}')
        for stage, count in tokens.items():
            print(f'  tokens {stage}: ~{count}')
        print(f"Walk: {summary['pass']} passed, {summary['warn']} warned, {summary['fail']} failed")
    if summary['fail']:
        raise SystemExit(1)


# --- stage launcher ----------------------------------------------------------
# Run by the human (or a script), never by the agent: commit the approved artifacts,
# create the checkout, compile the stage rules there, and print the launch prompt.
STAGE_DIRS = {1: '01-plan', 2: '02-design', 3: '03-build', 4: '04-test', 5: '05-deploy', 6: '06-maintain'}
STAGE_WORDS = {1: 'plan', 2: 'design', 3: 'build', 4: 'test', 5: 'deploy', 6: 'maintain'}
TINK_TIMEOUT = 60


def run_git(cwd, *args):
    return subprocess.run(['git', '-C', str(cwd), *args], capture_output=True, text=True)


def git_line(result):
    lines = [line for line in (result.stderr + result.stdout).splitlines() if line.strip()]
    return lines[0] if lines else f'git exited {result.returncode}'


def stage_skillset(n):
    """Skillset named by the stage contract's Skills section, or None."""
    text = read_text_safe(ROOT / 'stages' / STAGE_DIRS[n] / 'CONTEXT.md')
    match = re.search(r'^## Skills\b(.*?)(?=^## |\Z)', text or '', re.M | re.S)
    named = re.search(r'Skillset:\s*`([a-z0-9]+-skillset)`', match.group(1)) if match else None
    return named.group(1) if named else None


def stage_entry_gates(path, run, n):
    profile = require_run(path)['profile']
    if n == 2 and profile == 'light':
        raise ValueError('light runs define in stage 1; the single definition gate is recorded as stage 3')
    needed = {1: [], 2: [1], 3: [3] if profile == 'light' else [1, 2]}.get(n) if n <= 3 else stages(path)
    for stage in needed:
        state = gate(path, stage)
        if state != 'approved':
            raise ValueError(f'cannot open stage {n}: the stage {stage} approval is {state}; record it with: '
                             f'sdlc.py decide {run} {stage} approved --reviewer NAME --source REF --reason TEXT')
    if n == 5:
        record_path = path / '04-test/output/verification.json'
        current = False
        try:
            record = read_json(record_path)
            current = (record.get('result') == 'passed' and evidence_matches(record, evidence_inputs(path))
                       and record['log'] == digest((record_path.parent / 'test-log.md').read_bytes()))
        except (ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError):
            pass
        if not current:
            raise ValueError('run verify first: stage 5 reviews current evidence '
                             f'(python3 _system/scripts/sdlc.py verify {run})')


def stage_identity_ok():
    for who in ('COMMITTER', 'AUTHOR'):
        name = os.environ.get(f'GIT_{who}_NAME') or run_git(ROOT, 'config', 'user.name').stdout.strip()
        email = os.environ.get(f'GIT_{who}_EMAIL') or run_git(ROOT, 'config', 'user.email').stdout.strip()
        if not (name and email):
            return False
    return True


def require_tink_use(tink):
    """Refuse early, with the fix, when the installed tink predates `tink use` (which compiles the stage disciplines)."""
    try:
        probe = subprocess.run([tink, 'use', '--help'], capture_output=True, text=True, timeout=15)
    except (subprocess.TimeoutExpired, OSError):
        return
    if probe.returncode == 0:
        return
    try:
        version = subprocess.run([tink, '--version'], capture_output=True, text=True, timeout=15).stdout.strip() or 'the installed tink'
    except (subprocess.TimeoutExpired, OSError):
        version = 'the installed tink'
    raise ValueError(f'{version} has no `tink use` (it compiles the stage disciplines); upgrade tink, then re-run the same command')


def stage_seed_contract(value):
    """Return (resolved path, bytes) of the operator's confirmed Seed Me seed contract.

    Whether the file is really confirmed is the operator's call: nothing here can tell. What is enforced is the
    binding: the run keeps its own copy (runs/<run>/seed-contract.md); that copy's digest is recorded in run.json
    and hashed into approvals and verification."""
    target = Path(value)
    if not target.exists() and not target.is_absolute():
        raise ValueError(f'seed contract file not found: {value}')
    resolved = target.resolve()
    if not resolved.is_file():
        raise ValueError(f'seed contract must be an existing regular file: {value}')
    return resolved, resolved.read_bytes()


def stage_dirty(run):
    result = run_git(ROOT, 'status', '--porcelain', '--untracked-files=all')
    paths = [line[3:].split(' -> ')[-1].strip('"') for line in result.stdout.splitlines() if line[3:]]
    return [p for p in paths if not p.startswith(f'runs/{run}/') and p not in (f'runs/{run}', f'runs/.{run}.lock')]


def tracked_mode_mismatches():
    """Tracked executable-bit changes Git may hide when core.fileMode is false."""
    result = run_git(ROOT, 'ls-files', '--stage', '-z')
    if result.returncode:
        return []
    dirty = []
    for entry in result.stdout.split('\0'):
        metadata, separator, relative = entry.partition('\t')
        fields = metadata.split()
        if not separator or len(fields) != 3 or fields[2] != '0' or fields[0] not in ('100644', '100755'):
            continue
        if relative.startswith('runs/'):
            continue
        path = ROOT / relative
        try:
            actual_executable = bool(path.lstat().st_mode & 0o111)
        except OSError:
            continue  # Git's normal status check reports missing or inaccessible paths.
        if actual_executable != (fields[0] == '100755'):
            dirty.append(relative)
    return dirty


def prune_empty_parents(target, stop):
    """Remove empty directories git created above `target`, up to but never including `stop` (the deepest one that existed)."""
    parent = Path(target).parent
    while parent != stop and stop in parent.parents:
        try:
            parent.rmdir()
        except OSError:
            break
        parent = parent.parent


def candidate_dirty(run):
    """Uncommitted paths that are part of the verified candidate (what verify fingerprints), not disposable caches or generated rules."""
    dirty = []
    paths = stage_dirty(run)
    paths.extend(path for path in tracked_mode_mismatches() if path not in paths)
    for relative in paths:
        # Run evidence is committed by the launcher when needed and is excluded from
        # the verified candidate fingerprint; another run's working files are unrelated.
        if relative.startswith('runs/'):
            continue
        tracked = run_git(ROOT, 'ls-files', '--error-unmatch', '--', relative).returncode == 0
        if not tracked and disposable(relative):
            continue
        if relative == 'AGENTS.md' and tracked:
            head = run_git(ROOT, 'show', 'HEAD:AGENTS.md')
            if head.returncode == 0 and strip_generated_rules((ROOT / relative).read_bytes()) == strip_generated_rules(head.stdout.encode()):
                continue
        dirty.append(relative)
    return dirty


MIN_TINK_ROUTE = (0, 10, 0)


def tink_route_warning():
    """One actionable warning when the installed version is old or cannot be confirmed."""
    route = shutil.which('tink-route')
    if not route:
        return None
    try:
        result = subprocess.run([route, '--version'], capture_output=True, text=True, timeout=10)
    except (subprocess.TimeoutExpired, OSError):
        return None
    out = result.stdout.strip() or result.stderr.strip()
    match = re.search(
        r'(?<![A-Za-z0-9])v?(\d+)\.(\d+)(?:\.(\d+))?'
        r'([.-]?(?:rc|a|b|dev|alpha|beta|pre|preview)\.?[0-9A-Za-z.-]*)?'
        r'(?:\+[0-9A-Za-z.-]+)?(?![A-Za-z0-9.+-])', out, re.I)
    upgrade = 'upgrade: pipx install --force git+https://github.com/jon-devlapaz/tink-route.git'
    if not match:
        detail = out[:120] if out else f'exit code {result.returncode} with no version output'
        return ('warning: could not determine whether tink-route supports whole-library routing '
                f'(version output: {detail}); {upgrade}')
    version = match.group(0)
    parts = tuple(int(match.group(index) or 0) for index in (1, 2, 3))
    prerelease = bool(match.group(4))
    if parts >= MIN_TINK_ROUTE and not prerelease:
        return None
    reason = (f'is a prerelease of {".".join(map(str, parts))}' if prerelease and parts >= MIN_TINK_ROUTE
              else f'is older than {".".join(map(str, MIN_TINK_ROUTE))}')
    return (f'warning: tink-route {version} {reason} and may not support whole-library routing; '
            f'upgrade: pipx install --force git+https://github.com/jon-devlapaz/tink-route.git')


PICK_LIMIT = 120000
PICK_TIMEOUT = 120


def stage_pick_document(path, n):
    """The stage's input document: the confirmed seed contract, intent, spec/brief, or review findings; None when absent."""
    metadata = require_run(path)
    if n == 1:
        bound = metadata.get('seed_contract')
        if not bound:
            return None
        found = Path(bound['path'])
        return found if found.is_absolute() else ROOT / found
    relative = {2: '01-plan/output/intent.md', 6: '05-deploy/output/REVIEW-findings.md',
                3: 'brief.md' if metadata['profile'] == 'light' else '02-design/output/spec.md'}
    relative[5] = relative[3]
    found = path / relative[n] if n in relative else None
    return found if found is not None and found.is_file() else None


def stage_pick(path, run, n, check):
    """Route the whole input document once at stage open. Returns (line to print or None, launch-prompt sentence or None).

    Never blocks the stage: every failure is a printed skip and a receipt under runs/<run>/skills/."""
    document, route = stage_pick_document(path, n), shutil.which('tink-route')
    if document is None or not route:
        return None, None
    try:
        shown = str(document.relative_to(ROOT))
    except ValueError:
        shown = str(document)
    if check:
        return f'Would pick a skill from {shown}', None
    raw = document.read_bytes()
    text = raw.decode('utf-8', errors='replace')
    receipt = {'stage': n, 'document': shown, 'sha256': digest(raw), 'chars': len(text)}
    line = sentence = None
    if len(text) > PICK_LIMIT:
        receipt['status'] = 'skipped'
        line = f'skill pick: skipped (document is {len(text)} characters; limit {PICK_LIMIT})'
    else:
        try:
            done = subprocess.run([route, '--pick', '--json', '--anywhere', text], capture_output=True, text=True, timeout=PICK_TIMEOUT)
            code, out = done.returncode, done.stdout
        except (subprocess.TimeoutExpired, OSError) as error:
            code, out = -1, str(error)
        try:
            picked = json.loads(out)
        except ValueError:
            picked = {}
        if not isinstance(picked, dict):
            picked = {}
        if code == 0 and picked.get('status') == 'routed' and picked.get('winner'):
            receipt.update(status='routed', winner=picked['winner'], confidence=picked.get('confidence'))
            sentence = (f" Stage-open skill pick: {picked['winner']} (confidence {picked.get('confidence')}); "
                        f"read it before relying on it: tink mount {picked['winner']} --json --payload.")
        elif code == 1:
            receipt['status'] = 'none'
            line = 'skill pick: no specialist skill applies'
        else:
            receipt['status'] = 'error'
            reason = picked.get('reason')
            reason = ' '.join(''.join(c if c.isprintable() else ' ' for c in reason).split())[:120] if isinstance(reason, str) else ''
            if reason:
                receipt['reason'] = reason
            suffix = f': {reason}' if reason else ''
            line = (f'skill pick: skipped (router exited {code}{suffix})' if code > 0 else
                    'skill pick: skipped (router output unreadable)' if code == 0 else 'skill pick: skipped (router did not complete)')
    with locked(path / '.writer-lock'):
        write_json(path / 'skills' / f'stage-{n}-pick.json', receipt)
    return line, sentence


def stage(args):
    run, n = args.run, args.n
    path = run_path(run)
    require_run(path)
    if n == 4:
        raise ValueError('stage 4 runs inside the stage-3 session (see stages/04-test/CONTEXT.md)')
    if n not in STAGE_DIRS:
        raise ValueError('stage must be one of 1, 2, 3, 5, 6')
    if args.worktree and args.here:
        raise ValueError('--worktree and --here are mutually exclusive')  # parser already rejects; defensive
    if args.seed_contract and n != 1:
        raise ValueError('--seed-contract applies to stage 1 only')
    top = run_git(ROOT, 'rev-parse', '--show-toplevel')
    if top.returncode or Path(top.stdout.strip()).resolve() != ROOT:
        raise ValueError('the scaffold must sit at the root of a git repository')
    stage_entry_gates(path, run, n)
    if n == 5:
        dirty = candidate_dirty(run)
        if dirty:
            mode_dirty = sorted(set(dirty) & set(tracked_mode_mismatches()))
            mode_note = ('; tracked executable mode is not represented in the Git index: ' + ', '.join(mode_dirty[:5])
                         if mode_dirty else '')
            raise ValueError('stage 5 reviews a commit, and these candidate changes are uncommitted: ' + ', '.join(dirty[:5])
                             + (f' (+{len(dirty) - 5} more)' if len(dirty) > 5 else '')
                             + mode_note + '; commit the candidate changes (verify again only if the code changed since it passed)')
    seed_source, seed_bytes = stage_seed_contract(args.seed_contract) if args.seed_contract else (None, None)
    seed_contract = f'runs/{run}/seed-contract.md' if seed_source else None
    skillset = stage_skillset(n)
    tink = shutil.which('tink') if skillset else None
    if tink:
        require_tink_use(tink)
    stage_dir = STAGE_DIRS[n]

    make_worktree = not args.here and (args.worktree or n in (3, 5))
    target = ROOT
    first_existing = ROOT
    detached = n != 3
    if make_worktree:
        target = Path(os.path.abspath(args.worktree)) if args.worktree else (
            ROOT.parent / (f'{ROOT.name}-review-{run}' if n == 5 else f'{ROOT.name}-{run}'))
        first_existing = target.parent
        while not first_existing.exists():
            first_existing = first_existing.parent
        if target.exists() or target.is_symlink():
            raise ValueError(f'worktree path already exists: {target} (remove it with: git worktree remove {target})')
        if not detached and run_git(ROOT, 'show-ref', '--verify', '--quiet', f'refs/heads/{run}').returncode == 0:
            raise ValueError(f"branch '{run}' already exists (remove it with: git branch -D {run}, after git worktree remove on any checkout using it)")
    if seed_contract and not args.check:
        with locked(path / '.writer-lock'):
            copy = ROOT / seed_contract
            previous = read_json(path / 'run.json').get('seed_contract', {})
            if seed_source != copy.resolve():
                copy.write_bytes(seed_bytes)
            source = previous.get('source', str(seed_source)) if seed_source == copy.resolve() else str(seed_source)
            write_json(path / 'run.json', {**read_json(path / 'run.json'),
                                            'seed_contract': {'path': seed_contract, 'sha256': digest(seed_bytes), 'source': source}})
    pick_line, pick_sentence = stage_pick(path, run, n, args.check)
    pending = bool(run_git(ROOT, 'status', '--porcelain', '--', f'runs/{run}').stdout.strip()) if make_worktree else False
    if pending and not stage_identity_ok():
        raise ValueError('no committer identity configured; set user.name and user.email (or GIT_COMMITTER_*) so the launcher can commit '
                         f'runs/{run}')
    notice = None if skillset else f'skills: skipped (stages/{stage_dir}/CONTEXT.md names no skillset)'
    if skillset and not tink:
        notice = 'skills: skipped (tink not installed); the agent will run without stage disciplines'
    snapshot_arg = f'runs/{run}/{stage_dir}'
    prompt = f'Begin stage {n} ({STAGE_WORDS[n]}) of SDLC run `{run}`.'
    if seed_contract:
        prompt += f' The operator-confirmed seed contract is at {seed_contract}; it is input, not authorization.'
    prompt += pick_sentence or ''
    warn = stage_dirty(run) if make_worktree else []
    warning = ('warning: not carried into the new checkout: ' + ', '.join(warn[:5]) + (f' (+{len(warn) - 5} more)' if len(warn) > 5 else '')
               if warn else None)

    if args.check and pick_line:
        print(pick_line)
    if args.check:
        if seed_contract and seed_source != (ROOT / seed_contract).resolve():
            print(f'Would copy {seed_source} to {seed_contract}')
        print(f'Would commit: {"yes" if pending else "no"}')
        how = f'branch {run}' if not detached else 'detached'
        print(f'Would create worktree: {target} ({how})' if make_worktree else 'Would create worktree: none')
    else:
        committed = None
        if pending:
            message = f'stage {n} open: {run} (approved artifacts and receipts)'
            added = run_git(ROOT, 'add', '-A', '--', f'runs/{run}')
            if added.returncode:
                raise ValueError(f'git add failed: {git_line(added)}')
            done = run_git(ROOT, 'commit', '-q', '-m', message, '--', f'runs/{run}')
            if done.returncode:
                raise ValueError(f'git commit failed: {git_line(done)}')
            committed = (run_git(ROOT, 'rev-parse', '--short', 'HEAD').stdout.strip(), message)
        if make_worktree:
            command = ['worktree', 'add', '--detach', str(target), 'HEAD'] if detached else ['worktree', 'add', '-b', run, str(target), 'HEAD']
            created = run_git(ROOT, *command)
            if created.returncode:
                if not detached:
                    run_git(ROOT, 'branch', '-D', run)
                prune_empty_parents(target, first_existing)
                raise ValueError(f'worktree creation failed: {git_line(created)}'
                                 + (f'\nalready committed {committed[0]}: {committed[1]}' if committed else ''))
    if args.check:
        print(f'Would run: tink use {skillset} --snapshot {snapshot_arg}' if tink else notice)
    elif tink:
        try:
            used = subprocess.run([tink, 'use', skillset, '--snapshot', snapshot_arg], cwd=target, capture_output=True,
                                  text=True, timeout=TINK_TIMEOUT)
            failure = None if used.returncode == 0 else next((l for l in used.stderr.splitlines() if l.strip()), f'tink use exited {used.returncode}')
        except (subprocess.TimeoutExpired, OSError) as error:
            failure = f'tink use did not complete: {error}'
        if failure:
            print(failure, file=sys.stderr)
            print('stage not opened: fix the skillset problem above', file=sys.stderr)
            if make_worktree:
                removed = run_git(ROOT, 'worktree', 'remove', '--force', str(target))
                if removed.returncode == 0 and not detached:
                    run_git(ROOT, 'branch', '-D', run)
                if removed.returncode == 0:
                    prune_empty_parents(target, first_existing)
                print(f'worktree removed; re-run the same command after fixing it' if removed.returncode == 0
                      else f'worktree left at {target}: {git_line(removed)}', file=sys.stderr)
            raise SystemExit(1)
        print(f'skills: compiled {skillset} into {target}')
    else:
        print(notice)
    if warning:
        print(warning)
    if pick_line and not args.check:
        print(pick_line)
    if not args.check and (stale_route := tink_route_warning()):
        print(stale_route)
    print(f'Checkout: {target}')
    print('Launch prompt (start a NEW session there):')
    print(prompt)


class ShortErrorParser(argparse.ArgumentParser):
    def error(self, message):
        self.exit(2, f'sdlc.py: error: {message}\n'
                     'Commands: new, status, decide, verify, mark, lock-tests, skills, stage, walk. See _system/SDLC.md.\n')


def main():
    parser = ShortErrorParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    new = commands.add_parser('new')
    new.add_argument('run')
    new.add_argument('--profile', choices=['light', 'full'], default='light')
    new.add_argument('--kind', choices=['feature', 'bug'], default='feature')
    state = commands.add_parser('status')
    state.add_argument('run', nargs='?')
    decision = commands.add_parser('decide')
    decision.add_argument('run')
    decision.add_argument('stage', type=int, choices=[1, 2, 3])
    decision.add_argument('decision', choices=['approved', 'changes-requested'])
    for flag in ['reviewer', 'source', 'reason']:
        decision.add_argument('--' + flag, required=True)
    verification = commands.add_parser('verify')
    verification.add_argument('run')
    marking = commands.add_parser('mark')
    marking.add_argument('run')
    marking.add_argument('item')
    marking.add_argument('result', choices=['passed', 'failed'])
    marking.add_argument('--evidence', required=True)
    baseline = commands.add_parser('lock-tests')
    baseline.add_argument('run')
    baseline.add_argument('paths', nargs='+')
    baseline.add_argument('--source', required=True)
    baseline.add_argument('--failure-evidence', required=True)
    skill = commands.add_parser('skills')
    skill.add_argument('tool', choices=list(TOOL_ACCEPTABLE_CODES))
    skill.add_argument('arguments', nargs=argparse.REMAINDER)
    launch = commands.add_parser('stage', help='open a stage: commit, checkout, compile rules, print the launch prompt')
    launch.add_argument('run')
    launch.add_argument('n', type=int)
    where = launch.add_mutually_exclusive_group()
    where.add_argument('--worktree', metavar='PATH')
    where.add_argument('--here', action='store_true')
    launch.add_argument('--seed-contract', metavar='PATH')
    launch.add_argument('--check', action='store_true', help='validate and print the plan; write nothing')
    lint = commands.add_parser('walk', help='structural walk lint (read-only)')
    lint.add_argument('--json', action='store_true')
    args = parser.parse_args()
    try:
        {'new': create, 'status': status, 'decide': decide, 'verify': verify, 'mark': mark, 'lock-tests': lock_tests, 'skills': skills, 'stage': stage, 'walk': walk}[args.command](args)
    except (ValueError, OSError, KeyError, subprocess.SubprocessError) as error:
        print(f'Error: {describe(error)}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
