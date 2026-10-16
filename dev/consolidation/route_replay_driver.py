#!/usr/bin/env python3

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import signal
import subprocess
import sys
import time
from typing import Any


sys.dont_write_bytecode = True
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"

IMAGE = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
TARGET = "x86_64-unknown-linux-musl"
CONFIG_PATH = pathlib.Path("dev/consolidation/winddown/gate-tools/semantic-route-matrix/evidence.json")
GENERATOR_PATH = pathlib.Path("dev/consolidation/winddown/gate-tools/semantic-route-matrix/generate.py")
SCHEMA_PATH = pathlib.Path("src/runtime/eval/indexed/full/schema.rs")
LOCK_PATH = pathlib.Path(".work/consolidation/infra/native-suite.lock")
CLEANUP_MARKER = pathlib.Path(".work/consolidation/infra/native-suite.cleanup-unresolved")
MODES = ("normal", "forced")
CASE_TIMEOUT_SECONDS = 150
OUTER_TERM_SECONDS = 10


class ReplayError(Exception):
    pass


def route_source_commit(config: dict[str, Any]) -> str:
    source_commit = config.get("route_witness_source_commit")
    if (
        not isinstance(source_commit, str)
        or re.fullmatch(r"[0-9a-f]{40}", source_commit) is None
    ):
        raise ReplayError("route-witness source commit must be a full lowercase Git commit ID")
    return source_commit


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def git_value(root: pathlib.Path, *args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def source_snapshot(root: pathlib.Path) -> dict[str, Any]:
    status = git_value(root, "status", "--porcelain=v1", "--untracked-files=all")
    return {
        "head": git_value(root, "rev-parse", "HEAD"),
        "tree": git_value(root, "rev-parse", "HEAD^{tree}"),
        "clean": status == "",
        "status": status,
    }


def read_json(path: pathlib.Path, helper: Any) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(), object_pairs_hook=helper.duplicate_json_object)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, helper.EvidenceError) as error:
        raise ReplayError(f"could not read JSON object {path}: {error}") from error
    if not isinstance(value, dict):
        raise ReplayError(f"expected a JSON object in {path}")
    return value


def load_helper(root: pathlib.Path) -> Any:
    path = root / GENERATOR_PATH
    spec = importlib.util.spec_from_file_location("semantic_route_matrix_generate", path)
    if spec is None or spec.loader is None:
        raise ReplayError(f"could not load route-witness validator: {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def build_case_rows(
    root: pathlib.Path,
    helper: Any,
    config: dict[str, Any],
    schema_rows: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    try:
        mappings = helper.validate_route_witness_mappings(root, config, schema_rows)
        helper.validate_scalar_route_oracle_mappings(config, set(mappings))
    except (OSError, helper.EvidenceError) as error:
        raise ReplayError(f"existing route-witness source validation failed: {error}") from error

    expression_by_tag = {
        row["tag"]: row for row in schema_rows if row.get("kind") == "Expr"
    }
    source_commit = route_source_commit(config)

    source_mappings = {
        test_id: mapping
        for test_id, mapping in mappings.items()
        if "child_source" in mapping
    }
    source_tags = {mapping["tag"] for mapping in source_mappings.values()}
    if source_tags != set(expression_by_tag):
        raise ReplayError(
            "source-validated child mappings do not cover every expression schema tag: "
            f"missing={sorted(set(expression_by_tag) - source_tags)}; "
            f"extra={sorted(source_tags - set(expression_by_tag))}"
        )

    rows: list[dict[str, Any]] = []
    for test_id, mapping in sorted(
        source_mappings.items(),
        key=lambda item: (expression_by_tag[item[1]["tag"]]["ordinal"], item[0]),
    ):
        schema = expression_by_tag.get(mapping["tag"])
        if schema is None:
            raise ReplayError(f"source mapping names a non-expression schema tag: {test_id}")
        source_bytes = helper.route_witness_source_bytes(root, mapping, source_commit)
        if source_bytes is None:
            raise ReplayError(
                f"canonical test source differs from pinned {source_commit}: {test_id}"
            )
        if mapping["child_source"].encode("utf-8") != source_bytes:
            raise ReplayError(f"configured child source bytes differ for {test_id}")
        mode_expectations = mapping["mode_expectations"]
        case_row: dict[str, Any] = {
            "test_id": test_id,
            "tag_name": mapping["tag"],
            "ordinal": schema["ordinal"],
            "source_bytes": source_bytes,
            "child_source_sha256": sha256_bytes(source_bytes),
            "mapping": mapping,
            "schema": schema,
        }
        for mode in MODES:
            expectation = mode_expectations[mode]
            route_id = expectation.get("route_id")
            if (
                not isinstance(route_id, str)
                or expectation.get("status") != "supported_by_artifact_format"
                or not helper.route_witness_route_allowed(schema, mode, route_id)
            ):
                raise ReplayError(f"unsupported {mode} route mapping for {test_id}")
            case_row[f"{mode}_route"] = route_id
        rows.append(case_row)
    return rows


def case_slug(row: dict[str, Any]) -> str:
    test_id = row["test_id"].encode("utf-8")
    return f"{row['ordinal']:03d}-{row['tag_name']}-{sha256_bytes(test_id)[:12]}"


def normalized_under(path: pathlib.Path, parent: pathlib.Path, *, strict_parent: bool = True) -> pathlib.Path:
    normalized = path.resolve(strict=False)
    normalized_parent = parent.resolve(strict=strict_parent)
    if not normalized.is_relative_to(normalized_parent):
        raise ReplayError(f"path must be under {parent}: {normalized}")
    return normalized


def validate_inputs(
    args: argparse.Namespace,
    config: dict[str, Any] | None = None,
) -> dict[str, Any]:
    root = args.repo.resolve(strict=True)
    if not (root / ".git").exists():
        raise ReplayError(f"repository metadata is missing under {root}")
    for name, value in (("--head", args.head), ("--tree", args.tree)):
        if re.fullmatch(r"[0-9a-f]{40}", value) is None:
            raise ReplayError(f"{name} must be a full lowercase Git object ID")
    actual = source_snapshot(root)
    if (actual["head"], actual["tree"]) != (args.head, args.tree) or not actual["clean"]:
        raise ReplayError(
            f"source freeze differs or is dirty: {actual['head']} {actual['tree']} clean={actual['clean']}"
        )
    if (args.product_head, args.product_tree) != (args.head, args.tree):
        raise ReplayError("verification products must be explicitly pinned to the selected source freeze")
    helper = load_helper(root)
    if config is None:
        config = read_json(root / CONFIG_PATH, helper)
    source_commit = route_source_commit(config)
    git_value(root, "cat-file", "-e", f"{source_commit}^{{commit}}")
    try:
        schema_rows = helper.parse_schema((root / SCHEMA_PATH).read_text())
        rows = build_case_rows(root, helper, config, schema_rows)
    except (OSError, helper.EvidenceError) as error:
        raise ReplayError(f"existing route-witness source validation failed: {error}") from error

    products: dict[str, dict[str, Any]] = {}
    for name, path_arg, digest_arg in (
        ("xsh", args.xsh, args.xsh_sha256),
        ("xsht", args.xsht, args.xsht_sha256),
    ):
        expected_path = root / ".work/target-final-canonical" / TARGET / "verification" / name
        product_arg = path_arg if path_arg.is_absolute() else root / path_arg
        if product_arg.is_symlink():
            raise ReplayError(f"{name} product path must not be a symlink: {product_arg}")
        product_path = product_arg.resolve(strict=True)
        if product_path != expected_path or not product_path.is_file() or not os.access(product_path, os.X_OK):
            raise ReplayError(f"{name} product must be the current root verification binary: {expected_path}")
        actual_digest = sha256_file(product_path)
        if re.fullmatch(r"[0-9a-f]{64}", digest_arg) is None or actual_digest != digest_arg:
            raise ReplayError(f"{name} product SHA-256 differs: got {actual_digest}")
        products[name] = {"path": product_path, "sha256": actual_digest}

    final_gate_root = root / ".work/final-review-evidence/final-correctness-gates"
    if (root / ".work").is_symlink() or (root / ".work/final-review-evidence").is_symlink() or final_gate_root.is_symlink():
        raise ReplayError("final-correctness-gates output parents must not be symlinks")
    output_arg = args.output_root if args.output_root.is_absolute() else root / args.output_root
    output_root = normalized_under(output_arg, final_gate_root, strict_parent=False)
    if output_root.parent != final_gate_root.resolve(strict=False):
        raise ReplayError("receipt output must be one fresh run directory under final-correctness-gates")
    if output_root.name != args.run_name:
        raise ReplayError("--run-name must match the fresh receipt output directory name")
    if output_root.exists() or output_root.is_symlink():
        raise ReplayError(f"receipt output root must not exist: {output_root}")
    if any(":" in str(path) for path in (root, output_root)):
        raise ReplayError("Docker bind paths cannot contain a colon")
    try:
        subprocess.run(
            ["git", "check-ignore", "--quiet", "--no-index", ".work/final-review-evidence/final-correctness-gates"],
            cwd=root,
            check=True,
        )
    except subprocess.CalledProcessError as error:
        raise ReplayError("final-correctness-gates output must be ignored by .git/info/exclude") from error

    return {
        "root": root,
        "head": actual["head"],
        "tree": actual["tree"],
        "helper": helper,
        "config": config,
        "source_commit": source_commit,
        "rows": rows,
        "products": products,
        "output_root": output_root,
    }


def print_plan(validated: dict[str, Any]) -> None:
    root = validated["root"]
    print("execution=not_started")
    print("route_credit=none")
    print(f"head={validated['head']}")
    print(f"tree={validated['tree']}")
    print(f"pinned_child_source={validated['source_commit']}")
    print(f"xsh={validated['products']['xsh']['path']}")
    print(f"xsh_sha256={validated['products']['xsh']['sha256']}")
    print(f"xsht={validated['products']['xsht']['path']}")
    print(f"xsht_sha256={validated['products']['xsht']['sha256']}")
    print(f"receipt={validated['output_root'] / 'receipt.json'}")
    print(f"image={IMAGE}; platform=linux/amd64; cpus=8; memory=16g; swap=0")
    print(f"root_mount={root}:/repo:ro")
    print(f"output_mount={validated['output_root']}:/scratch:rw; TMPDIR=/scratch")
    print(f"native_lock={root / LOCK_PATH}")
    print(
        f"tests={len(validated['rows'])}; modes={len(MODES)}; "
        f"invocations={len(validated['rows']) * len(MODES)}; "
        "xsht_args=test --exact ID; timeout=150s"
    )
    print("test_id\ttag\tordinal\tmode\texpected_route\tchild_source_sha256")
    for row in validated["rows"]:
        for mode in MODES:
            print(
                f"{row['test_id']}\t{row['tag_name']}\t{row['ordinal']}\t{mode}\t"
                f"{row[f'{mode}_route']}\t{row['child_source_sha256']}"
            )


def write_json(path: pathlib.Path, value: Any) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)


def terminate_process(process: subprocess.Popen[bytes]) -> tuple[bytes, bytes]:
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        return process.communicate(timeout=OUTER_TERM_SECONDS)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        return process.communicate()


def stop_container(cid_file: pathlib.Path) -> dict[str, Any]:
    try:
        container_id = cid_file.read_text().strip()
    except OSError as error:
        return {"resolved": False, "error": str(error)}
    if not re.fullmatch(r"[0-9a-fA-F]{12,64}", container_id):
        return {"resolved": False, "error": "container ID is missing or malformed"}
    commands = [
        ["docker", "stop", "--time", "5", container_id],
        ["docker", "rm", "--force", container_id],
        ["docker", "ps", "--all", "--no-trunc", "--filter", f"id={container_id}", "--format", "{{.ID}}"],
    ]
    results = []
    for command in commands:
        try:
            result = subprocess.run(command, capture_output=True, text=True, timeout=15, check=False)
            results.append({"argv": command, "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
        except (OSError, subprocess.TimeoutExpired) as error:
            results.append({"argv": command, "error": str(error)})
    presence = results[-1]
    resolved = (
        "exit_code" in presence
        and presence["exit_code"] == 0
        and not presence.get("stdout", "").strip()
    )
    return {"resolved": resolved, "container_id": container_id, "commands": results}


def native_command(
    validated: dict[str, Any],
    row: dict[str, Any],
    mode: str,
    witness_path: pathlib.Path,
    cid_file: pathlib.Path,
    container_name: str,
) -> list[str]:
    root = validated["root"]
    output_root = validated["output_root"]
    test_id = row["test_id"]
    target_root = validated["products"]["xsh"]["path"].parents[2]
    xsh_relative = validated["products"]["xsh"]["path"].relative_to(target_root).as_posix()
    xsht_relative = validated["products"]["xsht"]["path"].relative_to(target_root).as_posix()
    container_witness_path = "/scratch/" + witness_path.relative_to(output_root).as_posix()
    inner_env = [
        "/usr/bin/env",
        f"XSHT_INTERNAL_ROUTE_WITNESS_TEST_ID={test_id}",
        f"XSHT_INTERNAL_ROUTE_WITNESS_OUTPUT_DIR={container_witness_path}",
        f"XSH_BIN=/artifacts/target/{xsh_relative}",
        f"CARGO_BIN_EXE_xsh=/artifacts/target/{xsh_relative}",
    ]
    if mode == "forced":
        inner_env.append("XSH_INTERNAL_TEST_FORCE_FRAMES=1")
    inner_env.extend([f"/artifacts/target/{xsht_relative}", "test", "--exact", test_id])
    return [
        "docker",
        "run",
        "--rm",
        "--init",
        "--cidfile",
        str(cid_file),
        "--name",
        container_name,
        "--platform",
        "linux/amd64",
        "--network",
        "none",
        "--cpus=8",
        "--memory=16g",
        "--memory-swap=16g",
        "--user",
        f"{os.getuid()}:{os.getgid()}",
        "--env",
        "HOME=/scratch/home",
        "--env",
        "TMPDIR=/scratch",
        "--env",
        "PATH=/usr/local/bin:/usr/bin:/bin",
        "--env",
        "LC_ALL=C",
        "--env",
        "TZ=UTC",
        "--env",
        "NO_COLOR=1",
        "--env",
        "LD_PRELOAD=",
        "--volume",
        f"{root}:/repo:ro",
        "--volume",
        f"{target_root}:/artifacts/target:ro",
        "--volume",
        f"{output_root}:/scratch:rw",
        "--workdir",
        "/repo",
        IMAGE,
        *inner_env,
    ]


def run_case(
    validated: dict[str, Any],
    receipt: dict[str, Any],
    row: dict[str, Any],
    mode: str,
    run_name: str,
) -> bool:
    root = validated["root"]
    output_root = validated["output_root"]
    slug = case_slug(row)
    case_dir = output_root / "probes" / slug / mode
    witness_dir = case_dir / "witness"
    witness_dir.mkdir(parents=True)
    (output_root / "home").mkdir(exist_ok=True)
    (output_root / "logs").mkdir(exist_ok=True)
    (output_root / "container-ids").mkdir(exist_ok=True)
    step_id = f"{run_name}-{slug}-{mode}"
    container_name = f"route-{run_name}-{slug}-{mode}"
    cid_file = output_root / "container-ids" / f"{step_id}.cid"
    log_path = output_root / "logs" / f"{step_id}.log"
    command = native_command(validated, row, mode, witness_dir, cid_file, container_name)
    before = source_snapshot(root)
    if before["head"] != receipt["source_expected"]["commit"] or before["tree"] != receipt["source_expected"]["tree"] or not before["clean"]:
        raise ReplayError(f"source changed before {step_id}: {before}")
    receipt["source_checks"].append(
        {"where": f"before:{step_id}", **before}
    )
    current_xsh_hash = sha256_file(validated["products"]["xsh"]["path"])
    current_xsht_hash = sha256_file(validated["products"]["xsht"]["path"])
    if current_xsh_hash != receipt["hashes"]["binaries_before_tests"]["xsh-verification"]:
        raise ReplayError(f"xsh verification hash changed before {step_id}")
    if current_xsht_hash != receipt["hashes"]["binaries_before_tests"]["xsht-verification"]:
        raise ReplayError(f"xsht verification hash changed before {step_id}")

    started = utc_now()
    start_time = time.monotonic()
    host_environment = dict(os.environ)
    host_environment.pop("LD_PRELOAD", None)
    stdout = b""
    stderr = b""
    exit_code = 127
    timed_out = False
    interrupted = False
    cleanup: dict[str, Any] | None = None
    try:
        process = subprocess.Popen(
            command,
            cwd=root,
            env=host_environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
        try:
            stdout, stderr = process.communicate(timeout=CASE_TIMEOUT_SECONDS)
            exit_code = process.returncode
        except subprocess.TimeoutExpired:
            timed_out = True
            stdout, stderr = terminate_process(process)
            exit_code = 124
            cleanup = stop_container(cid_file)
        except KeyboardInterrupt:
            interrupted = True
            stdout, stderr = terminate_process(process)
            exit_code = 130
            cleanup = stop_container(cid_file)
    except OSError as error:
        stderr = str(error).encode("utf-8", "replace")

    with log_path.open("wb") as log:
        log.write(("argv: " + json.dumps(command) + "\n\n[stdout]\n").encode())
        log.write(stdout)
        log.write(b"\n[stderr]\n")
        log.write(stderr)
        if cleanup is not None:
            log.write(("\n[cleanup]\n" + json.dumps(cleanup, sort_keys=True) + "\n").encode())

    after = source_snapshot(root)
    receipt["source_checks"].append({"where": f"after:{step_id}", **after})
    after_xsh_hash = sha256_file(validated["products"]["xsh"]["path"])
    after_xsht_hash = sha256_file(validated["products"]["xsht"]["path"])
    receipt["hashes"]["binaries_after_tests"] = {
        "xsh-verification": after_xsh_hash,
        "xsht-verification": after_xsht_hash,
    }
    step = {
        "id": step_id,
        "argv": command,
        "env": {},
        "cwd": str(root),
        "log": str(log_path.relative_to(root)),
        "started_utc": started,
        "exit_code": exit_code,
        "timed_out": timed_out,
        "duration_seconds": round(time.monotonic() - start_time, 3),
        "log_sha256": sha256_file(log_path),
        "finished_utc": utc_now(),
    }
    if cleanup is not None:
        step["cleanup"] = cleanup

    artifacts = sorted(witness_dir.rglob("route-witness.json"))
    case_ok = exit_code == 0 and len(artifacts) == 1
    artifact_copy: pathlib.Path | None = None
    if len(artifacts) == 1:
        artifact_copy = output_root / "artifacts" / slug / mode / "route-witness.json"
        artifact_copy.parent.mkdir(parents=True, exist_ok=True)
        artifact_bytes = artifacts[0].read_bytes()
        artifact_copy.write_bytes(artifact_bytes)
        try:
            artifact = json.loads(
                artifact_bytes,
                object_pairs_hook=validated["helper"].duplicate_json_object,
            )
        except (json.JSONDecodeError, validated["helper"].EvidenceError):
            artifact = {}
        source_hash = artifact.get("source_hash") if isinstance(artifact, dict) else None
        step["route_witness_artifact"] = {
            "path": str(artifact_copy.relative_to(output_root)),
            "sha256": sha256_bytes(artifact_bytes),
            "source_hash": source_hash,
        }
        case_ok = case_ok and validated["helper"].route_witness_artifact_matches(
            artifact_bytes,
            row["mapping"],
            row["schema"],
            mode,
            row["ordinal"],
            row["source_bytes"],
        )
    else:
        (case_dir / "artifact-paths.txt").write_text(
            "".join(f"{path.relative_to(case_dir)}\n" for path in artifacts)
        )
        case_ok = False
    if after["head"] != receipt["source_expected"]["commit"] or after["tree"] != receipt["source_expected"]["tree"] or not after["clean"]:
        case_ok = False
    if after_xsh_hash != current_xsh_hash or after_xsht_hash != current_xsht_hash:
        case_ok = False
    receipt["steps"].append(step)
    write_json(output_root / "receipt.json", receipt)
    print(
        f"{row['ordinal']:03d} {row['tag_name']} {row['test_id']} "
        f"{mode} outer={exit_code} artifacts={len(artifacts)}"
    )
    if cleanup is not None and not cleanup.get("resolved", False):
        marker_path = root / CLEANUP_MARKER
        write_json(
            marker_path,
            {
                "run_id": run_name,
                "step": step_id,
                "cidfile": str(cid_file),
                "container_cleanup": cleanup,
                "process_cleanup": {"resolved": process.poll() is not None if "process" in locals() else False},
            },
        )
        raise ReplayError(f"container cleanup is unresolved; marker retained at {marker_path}")
    if interrupted:
        raise ReplayError(f"invocation interrupted after cleanup: {step_id}")
    return case_ok


def execute(validated: dict[str, Any], run_name: str) -> int:
    root = validated["root"]
    lock_path = root / LOCK_PATH
    if not lock_path.is_file() or lock_path.is_symlink():
        raise ReplayError(f"shared native-suite lock is missing or unsafe: {lock_path}")
    cleanup_marker = root / CLEANUP_MARKER
    if cleanup_marker.exists():
        raise ReplayError(f"native-suite cleanup marker blocks execution: {cleanup_marker}")
    output_root = validated["output_root"]
    output_root.parent.mkdir(parents=True, exist_ok=True)
    if output_root.exists() or output_root.is_symlink():
        raise ReplayError(f"receipt output root must not exist: {output_root}")
    output_root.mkdir()
    for directory in ("probes", "artifacts", "logs", "home", "container-ids"):
        (output_root / directory).mkdir()

    receipt: dict[str, Any] = {
        "schema": 1,
        "status": "running",
        "started_utc": utc_now(),
        "source_expected": {"commit": validated["head"], "tree": validated["tree"]},
        "environment": {
            "target": TARGET,
            "image": IMAGE,
            "platform": "linux/amd64",
            "cpus": 8,
            "memory": "16g",
            "memory_swap": "disabled",
            "runtime_preload": "none",
        },
        "steps": [],
        "source_checks": [],
        "hashes": {
            "binaries_before_tests": {
                "xsh-verification": validated["products"]["xsh"]["sha256"],
                "xsht-verification": validated["products"]["xsht"]["sha256"],
            },
            "binaries_after_tests": {},
        },
    }
    write_json(output_root / "receipt.json", receipt)
    failure_count = 0
    try:
        with lock_path.open("r+") as lock:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
            current = validate_inputs_for_execution(validated)
            if current != (validated["head"], validated["tree"]):
                raise ReplayError("source changed between plan validation and acquiring native-suite.lock")
            image_result = subprocess.run(
                ["docker", "image", "inspect", "--format", "{{.Id}}", IMAGE],
                cwd=root,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
            if image_result.returncode != 0 or image_result.stdout.strip() != IMAGE:
                raise ReplayError(
                    f"pinned Docker image is missing or mismatched: {image_result.stdout.strip()} {image_result.stderr.strip()}"
                )
            for row in validated["rows"]:
                for mode in MODES:
                    if not run_case(validated, receipt, row, mode, run_name):
                        failure_count += 1
    except (Exception, KeyboardInterrupt) as error:
        receipt["failure"] = str(error)
        failure_count += 1
    finally:
        receipt["status"] = "completed" if failure_count == 0 else "failed-or-incomplete"
        receipt["finished_utc"] = utc_now()
        write_json(output_root / "receipt.json", receipt)
    print(f"receipt={output_root / 'receipt.json'}")
    print(
        f"completed_steps={len(receipt['steps'])}; "
        f"expected={len(validated['rows']) * len(MODES)}"
    )
    print(f"route_credit=none until accepted by the source-pinned evidence owner")
    if failure_count:
        print(f"failures={failure_count}; inspect receipt, logs, and preserved artifacts", file=sys.stderr)
        return 1
    return 0


def validate_inputs_for_execution(validated: dict[str, Any]) -> tuple[str, str]:
    root = validated["root"]
    actual = source_snapshot(root)
    if (actual["head"], actual["tree"]) != (validated["head"], validated["tree"]) or not actual["clean"]:
        raise ReplayError(f"source freeze changed before execution: {actual}")
    for name in ("xsh", "xsht"):
        product = validated["products"][name]
        if sha256_file(product["path"]) != product["sha256"]:
            raise ReplayError(f"{name} verification binary changed before execution")
    return actual["head"], actual["tree"]


def add_inputs(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--repo", type=pathlib.Path, required=True)
    parser.add_argument("--head", required=True)
    parser.add_argument("--tree", required=True)
    parser.add_argument("--product-head", required=True)
    parser.add_argument("--product-tree", required=True)
    parser.add_argument("--xsh", type=pathlib.Path, required=True)
    parser.add_argument("--xsh-sha256", required=True)
    parser.add_argument("--xsht", type=pathlib.Path, required=True)
    parser.add_argument("--xsht-sha256", required=True)
    parser.add_argument("--output-root", type=pathlib.Path, required=True)
    parser.add_argument("--run-name", required=True)


def main(config: dict[str, Any] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Replay every source-validated expression route oracle in the current "
            "evidence configuration."
        )
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    plan_parser = subparsers.add_parser("plan")
    add_inputs(plan_parser)
    execute_parser = subparsers.add_parser("execute")
    add_inputs(execute_parser)
    args = parser.parse_args()
    if re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,63}", args.run_name) is None:
        parser.error("--run-name must be a unique path-safe label of at most 64 characters")
    try:
        validated = validate_inputs(args, config)
        if args.command == "plan":
            print_plan(validated)
            return 0
        return execute(validated, args.run_name)
    except (ReplayError, OSError, subprocess.CalledProcessError) as error:
        print(str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
