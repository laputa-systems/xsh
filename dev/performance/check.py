#!/usr/bin/env python3
from __future__ import annotations

from collections.abc import Callable
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
ADAPTER_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ADAPTER_ROOT))
from dev import gate_source
from dev.performance import owned_containers as owned


class CheckError(ValueError):
    pass


class DockerCleanupError(CheckError):
    def __init__(self, message, *, child_status, stdout, stderr, cleanup, cleanup_error, resolution_error):
        self.child_status = child_status
        self.stdout = stdout
        self.stderr = stderr
        self.cleanup = cleanup
        self.cleanup_error = cleanup_error
        self.resolution_error = resolution_error
        self.resolution_notes = tuple(getattr(resolution_error, '__notes__', ()))
        super().__init__(
            f'{message}; child_status={child_status!r} stdout={stdout!r} stderr={stderr!r}; '
            f'cleanup={cleanup!r}; cleanup_error={cleanup_error!r}; resolution_error={resolution_error!r}; '
            f'resolution_notes={self.resolution_notes!r}'
        )


def require(condition, message):
    if not condition:
        raise CheckError(message)


def fields(value, expected, label):
    require(isinstance(value, dict) and set(value) == set(expected), f'{label}: expected fields {sorted(expected)}')


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f'duplicate JSON field: {key}')
        result[key] = value
    return result


def read_json(path):
    return json.loads(path.read_text(), object_pairs_hook=unique_object)


def canonical(value):
    require(isinstance(value, str) and value != '', 'path must be explicit')
    path = Path(value)
    require(path.is_absolute() and str(path) == os.path.normpath(value) and '..' not in path.parts, f'path must be absolute and contain no symlinks: {path}')
    return path


def git(root, *args):
    return gate_source.git(os.fsencode(root), *(os.fsencode(arg) for arg in args), timeout=15).decode().strip()


def owned_digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def _docker_cleanup_error(message, process, result, primary, cleanup, cleanup_error, resolution_error):
    return DockerCleanupError(
        message,
        child_status=result[0] if result is not None else getattr(process, 'returncode', None),
        stdout=result[1] if result is not None else getattr(primary, 'output', None),
        stderr=result[2] if result is not None else getattr(primary, 'stderr', None),
        cleanup=cleanup,
        cleanup_error=cleanup_error,
        resolution_error=resolution_error,
    )


def _preserve_primary_cleanup(primary, cleanup_failure):
    try:
        primary.cleanup_failure = cleanup_failure
    except (AttributeError, TypeError) as error:
        primary.add_note(f'could not attach structured cleanup failure: {error!r}')
    primary.add_note(str(cleanup_failure))


def _close_process_pipes(process):
    failures = []
    for name in ('stdout', 'stderr'):
        pipe = getattr(process, name)
        if pipe is not None:
            try:
                pipe.close()
            except BaseException as error:
                failures.append(error)
    return failures


def checked_docker(command, scratch, *, timeout_seconds, on_cleanup_verified: Callable[[], None] | None = None):
    directory = Path(tempfile.mkdtemp(prefix='adapter-', dir=scratch))
    cidfile = directory / 'container.cid'
    owner = directory.name
    argv = owned.owned_docker_command(command, cidfile=cidfile, owner=owner, stage='adapter')
    process = None
    primary = None
    result = None
    previous = owned.install_signal_handlers()
    try:
        process = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        stdout, stderr = process.communicate(timeout=timeout_seconds)
        result = process.returncode, stdout, stderr
    except BaseException as error:
        primary = error
    cleanup = None
    cleanup_error = None
    resolution_error = None
    try:
        if process is None:
            with owned.cleanup_window():
                try:
                    cleanup = owned.cleanup_cidfile_container(cidfile, owner=owner, stage='adapter')
                except BaseException as error:
                    cleanup_error = error
                if cleanup_error is None and cleanup.get('resolved') is True and on_cleanup_verified is not None:
                    try:
                        on_cleanup_verified()
                    except BaseException as error:
                        resolution_error = error
        else:
            try:
                cleanup = owned.cleanup_container_process(
                    process,
                    lambda: owned.cleanup_cidfile_container(cidfile, owner=owner, stage='adapter'),
                )
            except BaseException as error:
                cleanup_error = error
            for failure in _close_process_pipes(process):
                if cleanup_error is None:
                    cleanup_error = failure
                else:
                    cleanup_error.add_note(f'owned pipe close failed: {failure!r}')
            if cleanup_error is None and cleanup.get('resolved') is True and on_cleanup_verified is not None:
                try:
                    on_cleanup_verified()
                except BaseException as error:
                    resolution_error = error
    finally:
        owned.restore_signal_handlers(previous)
    if cleanup_error is not None:
        failure = _docker_cleanup_error(
            'owned container cleanup failed', process, result, primary, cleanup, cleanup_error, resolution_error,
        )
        if primary is not None:
            _preserve_primary_cleanup(primary, failure)
            raise primary
        raise failure from cleanup_error
    cleanup_unresolved = cleanup.get('resolved') is not True
    scratch_error = None
    if cleanup.get('resolved') is True:
        try:
            shutil.rmtree(directory)
        except BaseException as error:
            scratch_error = error

    def with_scratch_note(error):
        if scratch_error is not None:
            error.add_note(f'owned scratch removal failed at {directory}: {scratch_error!r}')
        return error

    if primary is not None:
        if cleanup_unresolved:
            failure = _docker_cleanup_error(
                f'cleanup unresolved; owned cidfile retained at {cidfile}',
                process, result, primary, cleanup, cleanup_error, resolution_error,
            )
            _preserve_primary_cleanup(primary, failure)
        if resolution_error is not None:
            failure = _docker_cleanup_error(
                'failed to resolve cleanup quarantine after verified container cleanup',
                process, result, primary, cleanup, cleanup_error, resolution_error,
            )
            _preserve_primary_cleanup(primary, failure)
        raise with_scratch_note(primary)
    if resolution_error is not None:
        failure = _docker_cleanup_error(
            'container cleanup verified but cleanup quarantine resolution failed',
            process, result, primary, cleanup, cleanup_error, resolution_error,
        )
        if isinstance(resolution_error, (owned.SignalReceived, KeyboardInterrupt)):
            _preserve_primary_cleanup(resolution_error, failure)
            raise with_scratch_note(resolution_error)
        raise with_scratch_note(failure) from resolution_error
    if cleanup_unresolved:
        raise _docker_cleanup_error(
            f'cleanup unresolved; owned cidfile retained at {cidfile}',
            process, result, primary, cleanup, cleanup_error, resolution_error,
        )
    if scratch_error is not None:
        raise scratch_error
    return result


def baseline_owner(config, relative, name):
    path = config.source_root / relative
    require(path.is_file() and path == path.resolve(), f'baseline owner unavailable: {path}')
    key = f'performance_{name}_{hashlib.sha256(str(config.source_root).encode()).hexdigest()}'
    if key not in sys.modules:
        sys.path.insert(0, str(path.parent))
        spec = importlib.util.spec_from_file_location(key, path)
        module = importlib.util.module_from_spec(spec)
        sys.modules[key] = module
        spec.loader.exec_module(module)
    return sys.modules[key]
