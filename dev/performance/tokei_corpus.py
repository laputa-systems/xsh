from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import selectors
import shutil
import stat
import subprocess
import sys
import tempfile
import time


UUTILS_COMMIT = 'e7c9f3194280835c4487c2945c68d5f01ccacc8d'
MIN_UUTILS_RUST_FILES = 500
MAX_TREE_ENTRIES = 16384
MAX_PROJECTION_NODES = 16384
MAX_SOURCE_FILES = 2048
MAX_FILE_BYTES = 2 * 1024 * 1024
MAX_SOURCE_BYTES = 32 * 1024 * 1024
MAX_REPORT_BYTES = 16 * 1024 * 1024
GIT_TIMEOUT_SECONDS = 60
MAX_GIT_STDERR_BYTES = 64 * 1024
MAX_SMALL_GIT_OUTPUT_BYTES = 64 * 1024
MAX_TREE_OUTPUT_BYTES = 16 * 1024 * 1024
MAX_OBJECT_HEADER_BYTES = 128


class CorpusError(ValueError):
    pass


def _require(condition, message):
    if not condition:
        raise CorpusError(message)


def _git_environment():
    environment = {key: value for key, value in os.environ.items() if not key.startswith('GIT_')}
    environment['GIT_NO_REPLACE_OBJECTS'] = '1'
    return environment


def _monotonic():
    return time.monotonic()


def _stop_process(child):
    if child.poll() is None:
        try:
            child.kill()
        except ProcessLookupError:
            pass
    try:
        child.wait(timeout=1)
    except subprocess.TimeoutExpired as error:
        raise CorpusError('could not reap Git child process') from error


def _run_bounded_process(argv, *, deadline, stdout_limit, input_bytes=b'', env=None):
    child = None
    selector = None
    stdout = bytearray()
    stderr = bytearray()
    input_offset = 0
    try:
        child = subprocess.Popen(
            argv, stdin=subprocess.PIPE if input_bytes else subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0,
            env=env if env is not None else _git_environment(),
        )
        selector = selectors.DefaultSelector()
        for stream, label, limit in (
            (child.stdout, 'stdout', stdout_limit),
            (child.stderr, 'stderr', MAX_GIT_STDERR_BYTES),
        ):
            os.set_blocking(stream.fileno(), False)
            selector.register(stream, selectors.EVENT_READ, (label, limit))
        if input_bytes:
            os.set_blocking(child.stdin.fileno(), False)
            selector.register(child.stdin, selectors.EVENT_WRITE, ('stdin', 0))

        while selector.get_map():
            remaining = deadline - _monotonic()
            if remaining <= 0:
                raise CorpusError('Git operation exceeded its deadline')
            events = selector.select(remaining)
            if not events:
                continue
            for key, _ in events:
                label, limit = key.data
                if label == 'stdin':
                    try:
                        input_offset += os.write(child.stdin.fileno(), input_bytes[input_offset:])
                    except BrokenPipeError:
                        input_offset = len(input_bytes)
                    if input_offset == len(input_bytes):
                        selector.unregister(child.stdin)
                        child.stdin.close()
                    continue
                buffer = stdout if label == 'stdout' else stderr
                chunk = os.read(key.fileobj.fileno(), min(65536, limit - len(buffer) + 1))
                if not chunk:
                    selector.unregister(key.fileobj)
                    key.fileobj.close()
                    continue
                if len(buffer) + len(chunk) > limit:
                    raise CorpusError(f'Git {label} exceeded its {limit}-byte limit')
                buffer.extend(chunk)

        remaining = deadline - _monotonic()
        if remaining <= 0:
            raise CorpusError('Git operation exceeded its deadline')
        try:
            status = child.wait(timeout=remaining)
        except subprocess.TimeoutExpired as error:
            raise CorpusError('Git operation exceeded its deadline') from error
        return bytes(stdout), bytes(stderr), status
    except BaseException as error:
        if child is not None:
            try:
                _stop_process(child)
            except BaseException as cleanup_error:
                error.add_note(f'Git child cleanup failed: {cleanup_error}')
        raise
    finally:
        if selector is not None:
            selector.close()
        if child is not None:
            for stream in (child.stdin, child.stdout, child.stderr):
                if stream is not None and not stream.closed:
                    stream.close()


def _git(root, *args, deadline=None, stdout_limit=MAX_SMALL_GIT_OUTPUT_BYTES):
    deadline = deadline if deadline is not None else _monotonic() + GIT_TIMEOUT_SECONDS
    stdout, _, status = _run_bounded_process(
        ['git', '-C', str(root), *args], deadline=deadline, stdout_limit=stdout_limit,
    )
    _require(status == 0, f'git command failed: {args[0]}')
    return stdout


def _git_blob_files(root, candidates, *, deadline, max_file_bytes, max_total_bytes):
    request = b''.join(oid + b'\n' for _, _, oid, _ in candidates)
    output_limit = max_total_bytes + len(candidates) * MAX_OBJECT_HEADER_BYTES
    output, stderr, status = _run_bounded_process(
        ['git', '-C', str(root), 'cat-file', '--batch'],
        deadline=deadline, stdout_limit=output_limit, input_bytes=request,
    )
    _require(status == 0, 'git cat-file failed')
    _require(not stderr, 'git cat-file wrote diagnostics')
    files = {}
    offset = 0
    total = 0
    for relative, mode, oid, expected_size in candidates:
        _require(_monotonic() < deadline, 'Git operation exceeded its deadline')
        newline = output.find(b'\n', offset)
        _require(newline >= 0 and newline - offset <= MAX_OBJECT_HEADER_BYTES,
                 f'invalid Git blob header: {relative}')
        fields = output[offset:newline].split(b' ')
        _require(len(fields) == 3 and fields[0] == oid and fields[1] == b'blob',
                 f'missing Git blob: {relative}')
        size = int(fields[2])
        _require(size == expected_size and size <= max_file_bytes, f'Git blob size mismatch: {relative}')
        content_start = newline + 1
        content_end = content_start + size
        _require(content_end < len(output) and output[content_end:content_end + 1] == b'\n',
                 f'truncated Git blob: {relative}')
        content = output[content_start:content_end]
        object_id = hashlib.sha1(
            b'blob ' + str(size).encode('ascii') + b'\0' + content,
            usedforsecurity=False,
        ).hexdigest().encode('ascii')
        _require(object_id == oid, f'Git blob hash mismatch: {relative}')
        total += size
        _require(total <= max_total_bytes, f'source projection exceeds {max_total_bytes} bytes')
        files[relative] = (mode == b'100755', content)
        offset = content_end + 1
    _require(offset == len(output), 'git cat-file returned unexpected data')
    return files


def _read_bounded_regular(path, limit, label):
    _require(type(limit) is int and limit >= 0, f'invalid {label} byte limit')
    flags = os.O_RDONLY | os.O_NONBLOCK | getattr(os, 'O_NOFOLLOW', 0)
    descriptor = None
    try:
        descriptor = os.open(path, flags)
        before = os.fstat(descriptor)
        _require(stat.S_ISREG(before.st_mode), f'{label} must be a regular file: {path}')
        stream = os.fdopen(descriptor, 'rb')
        descriptor = None
        with stream:
            data = stream.read(limit + 1)
            after = os.fstat(stream.fileno())
            _require(len(data) <= limit, f'{label} exceeds its {limit}-byte limit: {path}')
            _require(
                before.st_size == after.st_size and before.st_mtime_ns == after.st_mtime_ns
                and before.st_ctime_ns == after.st_ctime_ns and len(data) == after.st_size,
                f'{label} changed while being read: {path}',
            )
        return data, after
    except OSError as error:
        raise CorpusError(f'cannot read {label}: {path}') from error
    finally:
        if descriptor is not None:
            os.close(descriptor)


def _safe_relative(raw):
    _require(raw and b'\0' not in raw and b'\\' not in raw, 'unsupported Git path')
    parts = raw.split(b'/')
    _require(all(part not in (b'', b'.', b'..') for part in parts), 'unsafe Git path')
    path = PurePosixPath(*(os.fsdecode(part) for part in parts))
    _require(not path.is_absolute() and '..' not in path.parts, 'unsafe Git path')
    return path.as_posix()


def _git_source_files(source_root, revision, *, max_entries=MAX_TREE_ENTRIES,
                      max_files=MAX_SOURCE_FILES, max_file_bytes=MAX_FILE_BYTES,
                      max_total_bytes=MAX_SOURCE_BYTES, min_files=MIN_UUTILS_RUST_FILES):
    _require(re.fullmatch('[0-9a-f]{40}', revision) is not None, 'revision must be a full lowercase Git commit ID')
    deadline = _monotonic() + GIT_TIMEOUT_SECONDS
    root = Path(source_root).resolve(strict=True)
    _require(root.is_dir(), 'source root must be a directory')
    top = os.fsdecode(_git(root, 'rev-parse', '--show-toplevel', deadline=deadline)).strip()
    _require(Path(top).resolve(strict=True) == root, 'source root must be a Git repository root')
    actual = os.fsdecode(_git(root, 'rev-parse', f'{revision}^{{commit}}', deadline=deadline)).strip()
    _require(actual == revision, 'pinned commit is unavailable')
    tree = _git(root, 'ls-tree', '-r', '-l', '-z', '--full-tree', revision,
                 deadline=deadline, stdout_limit=MAX_TREE_OUTPUT_BYTES)
    entries = tree.split(b'\0')
    _require(entries[-1] == b'', 'invalid Git tree listing')
    entries.pop()
    _require(len(entries) <= max_entries, f'Git tree exceeds {max_entries} entries')
    candidates = []
    for entry in entries:
        metadata, raw_path = entry.split(b'\t', 1)
        mode, kind, oid, raw_size = metadata.split()
        if mode == b'160000':
            raise CorpusError(f'Git submodule is not a source file: {os.fsdecode(raw_path)}')
        if mode not in (b'100644', b'100755') or kind != b'blob':
            continue
        relative = _safe_relative(raw_path)
        if any(part.startswith('.') for part in PurePosixPath(relative).parts):
            continue
        if not relative.lower().endswith('.rs'):
            continue
        size = int(raw_size)
        _require(size <= max_file_bytes, f'source file exceeds {max_file_bytes} bytes: {relative}')
        candidates.append((relative, mode, oid, size))
    _require(len(candidates) <= max_entries, 'source candidates exceed the tree-entry limit')
    _require(len(candidates) <= max_files, f'source candidates exceed {max_files} files')
    candidate_bytes = sum(row[3] for row in candidates)
    _require(candidate_bytes <= max_total_bytes, f'source candidates exceed {max_total_bytes} bytes')

    files = _git_blob_files(
        root, candidates, deadline=deadline, max_file_bytes=max_file_bytes,
        max_total_bytes=max_total_bytes,
    )
    _require(len(files) <= max_files, f'source projection exceeds {max_files} files')
    _require(len(files) >= min_files, f'Rust source projection has {len(files)} files; requires at least {min_files}')
    return files


def project_commit(source_root, projection_root, revision, *, max_entries=MAX_TREE_ENTRIES,
                   max_files=MAX_SOURCE_FILES, max_file_bytes=MAX_FILE_BYTES,
                   max_total_bytes=MAX_SOURCE_BYTES, min_files=MIN_UUTILS_RUST_FILES):
    files = _git_source_files(
        source_root, revision, max_entries=max_entries, max_files=max_files,
        max_file_bytes=max_file_bytes, max_total_bytes=max_total_bytes, min_files=min_files,
    )
    destination = Path(projection_root).absolute()
    _require(destination == destination.resolve(), 'projection path must not contain symlinks')
    _require(not destination.exists(), f'projection destination already exists: {destination}')
    parent = destination.parent
    _require(parent.is_dir() and parent == parent.resolve(), 'projection parent must exist and contain no symlinks')
    staging = Path(tempfile.mkdtemp(prefix='.tokei-projection-', dir=parent))
    try:
        for relative, (executable, content) in files.items():
            target = staging.joinpath(*PurePosixPath(relative).parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
            target.chmod(0o755 if executable else 0o644)
        os.replace(staging, destination)
    except BaseException as error:
        try:
            shutil.rmtree(staging)
        except BaseException as cleanup_error:
            error.add_note(f'projection staging cleanup failed: {cleanup_error}')
        raise
    return {'revision': revision, 'files': len(files), 'bytes': sum(len(data) for _, data in files.values())}


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        _require(key not in result, f'duplicate JSON field: {key}')
        result[key] = value
    return result


def _read_report(path):
    path = Path(path)
    data, _ = _read_bounded_regular(path, MAX_REPORT_BYTES, 'report')
    try:
        value = json.loads(data.decode('utf-8'), object_pairs_hook=_unique_object)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise CorpusError(f'invalid JSON report: {path}') from error
    _require(isinstance(value, dict), f'report root must be an object: {path}')
    return value


def _relative_report_path(name, root):
    _require(isinstance(name, str) and name, 'file report has no nonempty name')
    candidate = Path(name)
    if not candidate.is_absolute():
        candidate = root / candidate
    candidate = candidate.resolve(strict=False)
    _require(candidate.is_relative_to(root), f'file report escapes projection: {name}')
    relative = candidate.relative_to(root).as_posix()
    _require(relative and '..' not in PurePosixPath(relative).parts, f'invalid file report path: {name}')
    return relative


def _names(values, root, label):
    result = []
    for value in values:
        _require(isinstance(value, str), f'{label} file report name must be a string')
        result.append(_relative_report_path(value, root))
    _require(len(result) == len(set(result)), f'{label} contains duplicate file reports')
    return set(result)


def _xsh_names(report, root):
    total = report.get('Total')
    _require(isinstance(total, dict) and isinstance(total.get('children'), dict), 'XSH report has no Total.children file inventory')
    names = []
    for label, reports in total['children'].items():
        _require(isinstance(label, str) and isinstance(reports, list), 'invalid XSH language file inventory')
        for row in reports:
            _require(isinstance(row, dict), 'invalid XSH file report')
            names.append(row.get('name'))
    return _names(names, root, 'XSH')


def _tokei_names(report, root):
    names = []
    for label, language in report.items():
        _require(isinstance(label, str) and isinstance(language, dict), 'invalid Tokei language entry')
        reports = language.get('reports')
        _require(isinstance(reports, list), f'Tokei JSON has no per-file reports for {label}')
        for row in reports:
            _require(isinstance(row, dict), f'invalid Tokei file report for {label}')
            names.append(row.get('name'))
    return _names(names, root, 'Tokei')


def reconcile(source_root, projection_root, xsh_json, tokei_json, *, revision=UUTILS_COMMIT,
              max_entries=MAX_TREE_ENTRIES, max_files=MAX_SOURCE_FILES,
              max_file_bytes=MAX_FILE_BYTES, max_total_bytes=MAX_SOURCE_BYTES,
              min_files=MIN_UUTILS_RUST_FILES):
    expected_data = _git_source_files(
        source_root, revision, max_entries=max_entries, max_files=max_files,
        max_file_bytes=max_file_bytes, max_total_bytes=max_total_bytes, min_files=min_files,
    )
    root = Path(projection_root).resolve(strict=True)
    _require(root.is_dir(), 'projection must be a directory')
    expected = set(expected_data)
    actual = set()
    actual_bytes = 0
    nodes = 1
    pending = [root]
    while pending:
        directory = pending.pop()
        try:
            entries = os.scandir(directory)
        except OSError as error:
            raise CorpusError(f'cannot read projection directory: {directory}') from error
        with entries:
            for entry in entries:
                nodes += 1
                _require(nodes <= MAX_PROJECTION_NODES,
                         f'projection exceeds {MAX_PROJECTION_NODES} files and directories')
                path = Path(entry.path)
                if entry.is_symlink():
                    raise CorpusError(f'projection contains a symlink: {path}')
                if entry.is_dir(follow_symlinks=False):
                    pending.append(path)
                    continue
                _require(entry.is_file(follow_symlinks=False), f'projection contains a non-regular file: {path}')
                relative = path.relative_to(root).as_posix()
                actual.add(relative)
                _require(len(actual) <= max_files, f'projection exceeds {max_files} files')
                file_limit = min(max_file_bytes, max_total_bytes - actual_bytes)
                content, metadata = _read_bounded_regular(path, file_limit, f'projection file {relative}')
                actual_bytes += len(content)
                _require(actual_bytes <= max_total_bytes, f'projection exceeds {max_total_bytes} bytes')
                _require(relative in expected_data and content == expected_data[relative][1],
                         f'projection contents differ from pinned Git source: {relative}')
                _require(metadata.st_mode & 0o111 == (0o111 if expected_data[relative][0] else 0),
                         f'projection mode differs from pinned Git source: {relative}')
    _require(actual == expected, 'projection file set differs from pinned Git source projection')
    _require(actual_bytes == sum(len(data) for _, data in expected_data.values()), 'projection byte count differs from pinned Git source projection')
    xsh = _xsh_names(_read_report(xsh_json), root)
    tokei = _tokei_names(_read_report(tokei_json), root)
    result = {
        'revision': revision,
        'expected_files': len(expected),
        'xsh': {'selected_files': len(xsh), 'missing': sorted(expected - xsh), 'unexpected': sorted(xsh - expected)},
        'tokei': {'selected_files': len(tokei), 'missing': sorted(expected - tokei), 'unexpected': sorted(tokei - expected)},
    }
    result['exact'] = not (result['xsh']['missing'] or result['xsh']['unexpected']
                           or result['tokei']['missing'] or result['tokei']['unexpected'])
    return result


def _main(argv=None):
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest='command', required=True)
    project = commands.add_parser('project-uutils-rust')
    project.add_argument('--source-root', type=Path, required=True)
    project.add_argument('--output', type=Path, required=True)
    compare = commands.add_parser('reconcile-uutils-rust')
    compare.add_argument('--source-root', type=Path, required=True)
    compare.add_argument('--projection', type=Path, required=True)
    compare.add_argument('--xsh-json', type=Path, required=True)
    compare.add_argument('--tokei-json', type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        work_root = Path(__file__).resolve().parents[2] / '.work/tmp'
        if args.command == 'project-uutils-rust':
            output = args.output.absolute()
            _require(output.is_relative_to(work_root), 'projection output must be under repository .work/tmp')
            result = project_commit(args.source_root, output, UUTILS_COMMIT, min_files=MIN_UUTILS_RUST_FILES)
        else:
            result = reconcile(args.source_root, args.projection, args.xsh_json, args.tokei_json)
    except CorpusError as error:
        print(f'tokei-corpus: {error}', file=sys.stderr)
        return 2
    print(json.dumps(result, sort_keys=True, separators=(',', ':')))
    return 0 if args.command == 'project-uutils-rust' or result['exact'] else 1


if __name__ == '__main__':
    raise SystemExit(_main())
