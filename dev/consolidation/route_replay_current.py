#!/usr/bin/env python3

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import pathlib
import re
import sys
from types import ModuleType
from typing import Any


sys.dont_write_bytecode = True
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"

DRIVER_RELATIVE = pathlib.Path("dev/consolidation/evidence/routes-ed908/driver.py")
CONFIG_RELATIVE = pathlib.Path("dev/consolidation/winddown/gate-tools/semantic-route-matrix/evidence.json")
SOURCE_PIN_KEY = "route_witness_source_commit"


class CurrentReplayError(Exception):
    pass


def duplicate_json_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise CurrentReplayError(f"duplicate route configuration key: {key}")
        result[key] = value
    return result


def checked_source_file(root: pathlib.Path, relative: pathlib.Path) -> pathlib.Path:
    path = root
    for part in relative.parts:
        path /= part
        if path.is_symlink():
            raise CurrentReplayError(f"route replay source path contains a symlink: {path}")
    if not path.is_file():
        raise CurrentReplayError(f"route replay source file is missing: {path}")
    return path


def read_route_config(root: pathlib.Path) -> dict[str, Any]:
    config_path = checked_source_file(root, CONFIG_RELATIVE)
    try:
        value = json.loads(
            config_path.read_text(encoding="utf-8"),
            object_pairs_hook=duplicate_json_object,
        )
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise CurrentReplayError(
            f"could not read current route configuration: {error}"
        ) from error
    if not isinstance(value, dict):
        raise CurrentReplayError("current route configuration must be an object")
    return value


def configure_source_pin(driver: ModuleType, config: dict[str, Any]) -> str:
    source_commit = config.get(SOURCE_PIN_KEY)
    if (
        not isinstance(source_commit, str)
        or re.fullmatch(r"[0-9a-f]{40}", source_commit) is None
    ):
        raise CurrentReplayError("configured route source pin must be a full lowercase Git commit ID")
    driver.PINNED_SOURCE_COMMIT = source_commit
    return source_commit


def load_route_driver(root: pathlib.Path) -> ModuleType:
    driver_path = checked_source_file(root, DRIVER_RELATIVE)
    spec = importlib.util.spec_from_file_location(
        "routes_ed908_driver_current_replay", driver_path
    )
    if spec is None or spec.loader is None:
        raise CurrentReplayError(f"could not load retained route executor: {driver_path}")
    driver = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = driver
    spec.loader.exec_module(driver)
    return driver


def repository_root(arguments: list[str]) -> pathlib.Path:
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--repo", type=pathlib.Path, required=True)
    parsed, _ = parser.parse_known_args(arguments)
    return parsed.repo.resolve(strict=True)


def invoke_driver(driver: ModuleType, arguments: list[str]) -> int:
    original_argv = sys.argv
    sys.argv = [str(pathlib.Path(__file__).resolve()), *arguments]
    try:
        return driver.main()
    finally:
        sys.argv = original_argv


def main(arguments: list[str] | None = None) -> int:
    forwarded = list(sys.argv[1:] if arguments is None else arguments)
    help_requested = any(argument in {"-h", "--help"} for argument in forwarded)
    try:
        root = pathlib.Path(__file__).resolve().parents[2]
        if not help_requested:
            root = repository_root(forwarded)
        driver = load_route_driver(root)
        if not help_requested:
            configure_source_pin(driver, read_route_config(root))
    except (CurrentReplayError, OSError, ImportError) as error:
        print(f"current route replay: {error}", file=sys.stderr)
        return 2

    return invoke_driver(driver, forwarded)


if __name__ == "__main__":
    raise SystemExit(main())
