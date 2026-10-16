from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import sys
import tempfile
from typing import Any

sys.dont_write_bytecode = True
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"

HERE = pathlib.Path(__file__).resolve().parent
ROOT = pathlib.Path("/home/josh/d/laputa-systems/xsh")
SOURCE = HERE / "plan_source.py"
GENERATOR = HERE / "generate_manifest.py"


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def render_manifest() -> dict[str, Any]:
    sys.path.insert(0, str(HERE))
    from plan_source import plan_data

    plan = plan_data()
    environment = plan["environment"]
    environment.update({
        "native_runner_base_sha256": sha256(pathlib.Path(environment["native_runner_base_path"])),
        "final_runner_sha256": sha256(HERE / "run-native-final.sh"),
        "launch_sha256": sha256(HERE / "launch.py"),
        "runtime_cargo_forwarder_sha256": sha256(HERE / "cargo-forwarder.sh"),
        "runtime_rustc_wrapper_sha256": sha256(HERE / "rustc-jemalloc.sh"),
        "cleanup_self_test_sha256": sha256(HERE / "test_launcher_cleanup.py"),
        "manifest_source_sha256": sha256(SOURCE),
        "manifest_generator_sha256": sha256(GENERATOR),
    })
    return plan


def manifest_body_sha256(plan: dict[str, Any]) -> str:
    pinned_body = json.loads(json.dumps(plan))
    del pinned_body["environment"]["launch_sha256"]
    encoded = json.dumps(pinned_body, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args()
    work_root = ROOT / ".work"
    output = pathlib.Path(os.path.abspath(args.output))
    work_root = work_root.resolve(strict=True)
    if not output.is_relative_to(work_root) or output.exists() or output.is_symlink():
        parser.error("output must be a new file under repository .work")
    cursor = work_root
    for part in output.relative_to(work_root).parts[:-1]:
        cursor /= part
        if cursor.is_symlink():
            parser.error(f"output path contains a symlink: {cursor}")
    output.parent.mkdir(parents=True, exist_ok=True)
    plan = render_manifest()
    encoded = (json.dumps(plan, indent=2, sort_keys=True) + "\n").encode()
    temporary: pathlib.Path | None = None
    try:
        with tempfile.NamedTemporaryFile(dir=output.parent, prefix=output.name + ".", delete=False) as stream:
            temporary = pathlib.Path(stream.name)
            stream.write(encoded)
            stream.flush()
            os.fsync(stream.fileno())
        assert temporary is not None
        os.link(temporary, output)
        temporary.unlink()
        directory = os.open(output.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    print(json.dumps({
        "output": str(output),
        "manifest_body_sha256": manifest_body_sha256(plan),
        "manifest_source_sha256": plan["environment"]["manifest_source_sha256"],
    }, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
