#!/usr/bin/env python3
"""Resolve frozen compatibility IDs to their read-only upstream sources."""

from __future__ import annotations

import json
import os
from pathlib import Path, PurePosixPath

REPO = Path(__file__).resolve().parents[3]
LOCK = REPO / "dev/compat/upstream.lock.json"


def _lock() -> dict:
    return json.loads(LOCK.read_text())


def _root(suite: str, lock: dict) -> Path:
    if suite == "uutils":
        configured = os.environ.get("UUTILS_ROOT")
        root = Path(configured) if configured else REPO.parent / "ref/uutils-coreutils"
    elif suite == "gnu":
        configured = os.environ.get("GNU_ROOT")
        version = lock["gnu_coreutils"]["version"]
        root = Path(configured) if configured else REPO.parent / f"ref/gnu-coreutils-{version}"
    elif suite == "busybox":
        configured = os.environ.get("BUSYBOX_SOURCE_ROOT")
        parent = Path(configured) if configured else REPO / ".work/upstream/busybox"
        root = parent / lock["busybox"]["source_directory"]
    else:
        raise ValueError(f"unknown compatibility suite {suite!r}")

    if not root.is_dir():
        raise FileNotFoundError(f"{suite} reference checkout is missing: {root}")
    return root.resolve()


def _busybox_test_source(root: Path, util: str) -> Path:
    testsuite = root / "testsuite"
    test_file = testsuite / f"{util}.tests"
    test_directory = testsuite / util
    # Applet directories can hold binary fixtures beside the aggregate script.
    # Legacy-only utilities instead store one runnable test per directory entry.
    if test_file.is_file():
        return test_file
    if test_directory.is_dir():
        return test_directory
    raise FileNotFoundError(f"BusyBox source for {util!r} is missing under {testsuite}")


def reference_paths(suite: str, util: str) -> dict[str, Path]:
    """Return validated roots for one utility in a frozen suite.

    ``source`` is the pinned checkout. ``tests`` names the per-utility upstream
    source location (or its directory when GNU has one source per test).
    ``fixtures`` is present only when that uutils utility has its own fixtures.
    """
    lock = _lock()
    source = _root(suite, lock)

    if suite == "uutils":
        tests = source / "tests/by-util"
        test_module = tests / f"test_{util}.rs"
        if not test_module.is_file():
            raise FileNotFoundError(f"uutils source for {util!r} is missing: {test_module}")
        paths = {"source": source, "tests": test_module}
        fixtures = source / "tests/fixtures" / util
        if fixtures.is_dir():
            paths["fixtures"] = fixtures
        return paths

    if suite == "gnu":
        tests = source / "tests" / util
        if not tests.is_dir():
            raise FileNotFoundError(f"GNU tests for {util!r} are missing: {tests}")
        return {"source": source, "tests": tests}

    if suite == "busybox":
        testsuite = source / "testsuite"
        if not testsuite.is_dir():
            raise FileNotFoundError(f"BusyBox testsuite is missing: {testsuite}")
        tests = _busybox_test_source(source, util)
        return {"source": source, "tests": tests}

    raise ValueError(f"unknown compatibility suite {suite!r}")


def test_file(suite: str, util: str, suffix: str) -> str:
    """Return the validated upstream source path relative to its checkout.

    GNU freeze IDs name Automake ``.log`` files, while the read-only sources
    are ``.sh`` or ``.pl``. BusyBox IDs name individual cases in a per-utility
    testsuite source, so they resolve to that source file or directory.
    """
    paths = reference_paths(suite, util)
    source = paths["source"]

    if suite == "uutils":
        expected = f"test_{util}::"
        if not suffix.startswith(expected):
            raise ValueError(f"uutils origin {suffix!r} does not belong to {util!r}")
        return paths["tests"].relative_to(source).as_posix()

    prefix = f"{util}/"
    if not suffix.startswith(prefix) or len(suffix) == len(prefix):
        raise ValueError(f"{suite} origin {suffix!r} does not belong to {util!r}")
    name = PurePosixPath(suffix[len(prefix):])
    if name.is_absolute() or ".." in name.parts:
        raise ValueError(f"invalid {suite} origin path {suffix!r}")

    if suite == "gnu":
        if name.suffix != ".log":
            raise ValueError(f"GNU frozen test must name a .log file: {suffix!r}")
        stem = str(name)[:-len(".log")]
        tests = source / "tests" / util
        candidates = [tests / f"{stem}{ext}" for ext in (".sh", ".pl")]
        existing = [path for path in candidates if path.is_file()]
        if not existing:
            raise FileNotFoundError(f"GNU source for {suffix!r} is missing (.sh or .pl under {tests})")
        if len(existing) != 1:
            joined = ", ".join(str(path) for path in existing)
            raise ValueError(f"GNU source for {suffix!r} is ambiguous: {joined}")
        return existing[0].relative_to(source).as_posix()

    if suite == "busybox":
        expected = paths["tests"]
        return expected.relative_to(source).as_posix()

    raise ValueError(f"unknown compatibility suite {suite!r}")
