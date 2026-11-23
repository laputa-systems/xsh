import argparse
from collections import Counter, defaultdict
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys


# Rule | Role | Match (first match wins)
ROLE_RULES = (
    ('rejection', 'rejection fixture', 'rejected, invalid, reject or error fixture names; checker fixtures'),
    ('generated', 'generated', 'generated directory or generated filename'),
    ('benchmark', 'bench input', 'bench or dev/performance tree'),
    ('example', 'example', 'examples, docs, or showcase tree'),
    ('test-support', 'library', 'core/tests/support'),
    ('tests', 'test', 'test directories or test declarations'),
    ('installed', 'installed', 'package files or profile installation tree'),
    ('library', 'library', 'lib or stdlib tree'),
    ('entry', 'program', 'xsh shebang, main declaration, bin, boot, service or core entry'),
    ('workspace-module', 'library', 'known XSH or Laputa module directories'),
    ('workspace-script', 'program', 'known top-level and tooling scripts'),
)

MODULE = re.compile(r'^\s*(?:use|import)\s+([A-Za-z_][\w]*(?:\.[A-Za-z_][\w]*)*)\b', re.MULTILINE)
DYNAMIC = re.compile(r'\bmodule\.load\s*\([^\n]*')
TEST = re.compile(r'^\s*test\s+[\w-]+\s*\{', re.MULTILINE)
MAIN = re.compile(r'^\s*(?:cli\s+)?(?:proc\s+)?main(?:\s|\()', re.MULTILINE)


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], timeout=30)


def shebang(data):
    line = data.split(b'\n', 1)[0]
    if not line.startswith(b'#!'):
        return False
    try:
        words = shlex.split(line[2:].decode('utf-8'))
    except (UnicodeError, ValueError):
        return False
    if not words:
        return False
    if Path(words[0]).name == 'xsh':
        return True
    return Path(words[0]).name == 'env' and any(Path(word).name == 'xsh' for word in words[1:])


def classify(path, text, is_shebang):
    parts = path.lower().split('/')
    name = parts[-1]
    if ('tests/fixtures/' in path and any(x in parts for x in ('syntax', 'sema', 'checker', 'check'))) or any(x in parts for x in ('invalid', 'reject', 'rejected', 'errors')) or ('fixtures' in parts and any(x in name for x in ('invalid', 'reject', 'error'))):
        return 'rejection fixture', 'rejection'
    if 'generated' in parts or name.endswith('.generated.xsh'):
        return 'generated', 'generated'
    if parts[0] == 'bench' or path.startswith('dev/performance/'):
        return 'bench input', 'benchmark'
    if parts[0] in ('examples', 'docs', 'showcase') or 'examples' in parts or 'docs' in parts:
        return 'example', 'example'
    if path.startswith('core/tests/support/'):
        return 'library', 'test-support'
    if 'tests' in parts or TEST.search(text):
        return 'test', 'tests'
    if parts[0] == 'profiles' or (parts[0] == 'packages' and 'files' in parts):
        return 'installed', 'installed'
    if 'lib' in parts or parts[0] == 'stdlib':
        return 'library', 'library'
    if is_shebang or MAIN.search(text) or 'bin' in parts or 'boot' in parts or 'service' in parts or parts[0] == 'core':
        return 'program', 'entry'
    if parts[0] in ('guest', 'infra', 'installer', 'packages', 'pm', 'refresh', 'seed', 'system', 'trust', 'xinfra', 'xinit', 'dev'):
        return 'library', 'workspace-module'
    if len(parts) == 1 or parts[0] in ('tools', 'formal', 'qualification'):
        return 'program', 'workspace-script'
    return 'unclassified', 'no-match'


def config_values(root, key):
    config = root / 'xsht-config.ini'
    if not config.exists():
        return []
    values = []
    active = False
    for line in config.read_text().splitlines():
        if line.lstrip().startswith('#') or not line.strip():
            continue
        if line.startswith('['):
            break
        if not line[0].isspace() and '=' in line:
            name, value = line.split('=', 1)
            active = name.strip() == key
            if active and value.strip():
                values.append(value.strip())
        elif active and line[0].isspace():
            values.append(line.strip())
    return values


def matches(path, pattern):
    def match_parts(parts, patterns):
        if not patterns:
            return not parts
        if patterns[0] == '**':
            return match_parts(parts, patterns[1:]) or (bool(parts) and match_parts(parts[1:], patterns))
        return bool(parts) and bool(re.fullmatch(re.escape(patterns[0]).replace(r'\*', '.*'), parts[0])) and match_parts(parts[1:], patterns[1:])

    return match_parts(path.split('/'), pattern.split('/'))


def parser_path(root):
    for candidate in (root / 'target/verification/xsht', root / 'target/debug/xsht'):
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return str(candidate)
    found = shutil.which('xsht')
    if found is None:
        raise RuntimeError(f'xsht not found for {root}')
    return str(Path(found).resolve())


def parse(xsht, root, path):
    result = subprocess.run([xsht, 'ast', str(root / path)], cwd=root, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, timeout=30, check=False)
    return 'parsed' if result.returncode == 0 else 'parse-failed'


def census(roots, parse_file=parse):
    repos = []
    files = []
    contents = {}
    for root in roots:
        root = root.resolve(strict=True)
        head = git(root, 'rev-parse', 'HEAD').decode().strip()
        xsht = parser_path(root)
        repos.append({'root': str(root), 'head': head, 'xsht': xsht})
        exclusions = config_values(root, 'exclude')
        for raw in git(root, 'ls-files', '-z').split(b'\0'):
            if not raw:
                continue
            path = os.fsdecode(raw)
            try:
                data = (root / path).read_bytes()
            except OSError:
                if not path.endswith('.xsh'):
                    continue
                role, rule = classify(path, '', False)
                files.append({'repo': str(root), 'path': path, 'role': role, 'rule': rule,
                              'status': 'unreadable', 'sha256': None, 'imports': [], 'dynamic_imports': []})
                continue
            is_shebang = shebang(data)
            if not path.endswith('.xsh') and not is_shebang:
                continue
            text = data.decode('utf-8', errors='replace')
            role, rule = classify(path, text, is_shebang)
            status = 'excluded' if any(matches(path, p) for p in exclusions) else parse_file(xsht, root, path)
            record = {'repo': str(root), 'path': path, 'role': role, 'rule': rule, 'status': status,
                      'sha256': hashlib.sha256(data).hexdigest(), 'imports': [], 'dynamic_imports': []}
            files.append(record)
            contents[(str(root), path)] = text
    known = {str((Path(f['repo']) / f['path']).resolve()): f for f in files}
    for record in files:
        if record['status'] != 'parsed':
            continue
        root = Path(record['repo'])
        text = contents[(record['repo'], record['path'])]
        roots = [root / Path(record['path']).parent]
        roots += [root / p for p in os.environ.get('XSH_MODULE_PATH', '').split(os.pathsep) if p]
        config_dir = (root / record['path']).parent
        while config_dir != config_dir.parent and not (config_dir / 'xsht-config.ini').is_file():
            config_dir = config_dir.parent
        if (config_dir / 'xsht-config.ini').is_file():
            roots += [config_dir / p for p in (config_values(config_dir, 'module_path') or ['.'])]
        for module in MODULE.findall(text):
            target = None
            for base in roots:
                candidate = (base / (module.replace('.', '/') + '.xsh')).resolve()
                if candidate.is_file():
                    match = known.get(str(candidate))
                    if match is not None:
                        target = match['path'] if match['repo'] == str(root) else str(candidate)
                    break
            record['imports'].append({'module': module, 'resolved': target})
        record['dynamic_imports'] = DYNAMIC.findall(text)
    by_digest = defaultdict(list)
    counts = Counter()
    for record in files:
        counts[(record['repo'], record['role'], record['status'])] += 1
        if record['sha256'] is not None:
            by_digest[record['sha256']].append({'repo': record['repo'], 'path': record['path']})
    duplicates = [{'sha256': digest, 'files': paths} for digest, paths in sorted(by_digest.items()) if len(paths) > 1]
    summary = {'candidates': len(files), 'classified': sum(f['role'] != 'unclassified' for f in files),
               'unclassified': sum(f['role'] == 'unclassified' for f in files),
               'unresolved_imports': sum(i['resolved'] is None for f in files for i in f['imports']),
               'duplicate_groups': len(duplicates),
               'counts': [{'repo': repo, 'role': role, 'status': status, 'count': count}
                          for (repo, role, status), count in sorted(counts.items())]}
    return {'tool_version': 1, 'repos': repos, 'files': files, 'duplicates': duplicates, 'summary': summary}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--repo', action='append', required=True, type=Path)
    parser.add_argument('--json', required=True, type=Path)
    args = parser.parse_args()
    report = census(args.repo)
    args.json.resolve().write_text(json.dumps(report, indent=2) + '\n')
    summary = report['summary']
    for count in summary['counts']:
        print(f"{count['repo']} | {count['role']} | {count['status']}: {count['count']}")
    print(f"candidates: {summary['candidates']}; classified: {summary['classified']}; unclassified: {summary['unclassified']}")
    print(f"duplicate groups: {summary['duplicate_groups']}; unresolved imports: {summary['unresolved_imports']}")
    return 0 if not summary['unclassified'] else 1


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f'census: {error}', file=sys.stderr)
        sys.exit(2)
