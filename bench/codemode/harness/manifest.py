import hashlib
import json
import platform
import random
import subprocess
import time
from pathlib import Path

import prompts
from config import (
    LANE_BASE,
    MODEL,
    PI,
    ROOT,
    SX_REVISION,
    THINKING,
    TOOLCHAIN,
    TOOLCHAIN_SHA256,
    TS_BASE_IMAGE,
    TS_IMAGE_ID,
    TS_IMAGE_TAG,
    XSH_IMAGE,
)
from tasks import Task

SCHEMA = "codemode-manifest/1"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_toolchain() -> dict[str, str]:
    found = {name: sha256(TOOLCHAIN / name) for name in TOOLCHAIN_SHA256}
    if found != TOOLCHAIN_SHA256:
        raise RuntimeError(f"the frozen toolchain changed: {found} != {TOOLCHAIN_SHA256}")
    return found


def image_id(reference: str) -> str:
    completed = subprocess.run(
        ["docker", "image", "inspect", reference, "--format", "{{.Id}}"], capture_output=True, text=True, check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"image {reference} is not loaded: {completed.stderr.strip()}")
    return completed.stdout.strip()


def verify_images() -> dict[str, str]:
    ts = image_id(TS_IMAGE_TAG)
    if ts != TS_IMAGE_ID:
        raise RuntimeError(f"{TS_IMAGE_TAG} is {ts}, not the pinned {TS_IMAGE_ID}; rebuild bench/codemode/ts")
    return {"xsh": XSH_IMAGE, "xsh_id": image_id(XSH_IMAGE), "ts_tag": TS_IMAGE_TAG, "ts_id": ts, "ts_base": TS_BASE_IMAGE}


def pi_version() -> str:
    return subprocess.run([str(PI), "--version"], capture_output=True, text=True, check=False).stdout.strip()


def harness_revision() -> dict[str, str]:
    head = subprocess.run(["git", "-C", str(ROOT), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    dirty = subprocess.run(
        ["git", "-C", str(ROOT), "status", "--porcelain", "--", "bench/codemode"], capture_output=True, text=True,
    ).stdout.strip()
    return {"head": head, "bench_dirty": bool(dirty), "lane_base": LANE_BASE, "sx_revision": SX_REVISION}


def schedule(tasks: list[Task], conditions: list[str], samples: int, seed: int) -> list[dict]:
    cells = [
        {"cell": f"{t.id}/{c}/{s}", "task": t.id, "condition": c, "sample": s}
        for t in tasks for c in conditions for s in range(samples)
    ]
    random.Random(seed).shuffle(cells)
    for order, cell in enumerate(cells):
        cell["order"] = order
    return cells


def create(
    out: Path, phase: str, tasks: list[Task], conditions: list[str], samples: int, seed: int, jobs: int, split: str,
) -> dict:
    path = out / "manifest.json"
    if path.exists():
        raise FileExistsError(f"{path} exists; resume it with --resume or pick a new --out")
    out.mkdir(parents=True, exist_ok=True)
    contexts = {}
    for name in conditions:
        condition = prompts.CONDITIONS[name]
        text = prompts.context_text(condition)
        contexts[name] = None if text is None else {
            "file": condition.context, "sha256": hashlib.sha256(text.encode()).hexdigest(), "bytes": len(text.encode()),
        }
    manifest = {
        "schema": SCHEMA,
        "phase": phase,
        "split": split,
        "created": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "model": MODEL,
        "thinking_requested": THINKING,
        "pi_version": pi_version(),
        "toolchain": verify_toolchain(),
        "images": verify_images(),
        "harness": harness_revision(),
        "host": {"platform": platform.platform(), "python": platform.python_version()},
        "extraction_rule": prompts.EXTRACTION_RULE,
        "contexts": contexts,
        "tasks": {t.id: t.digest() for t in tasks},
        "samples": samples,
        "seed": seed,
        "jobs": jobs,
        "schedule": schedule(tasks, conditions, samples, seed),
    }
    path.write_text(json.dumps(manifest, indent=2))
    return manifest


def load(out: Path) -> dict:
    manifest = json.loads((out / "manifest.json").read_text())
    if manifest.get("schema") != SCHEMA:
        raise ValueError(f"{out / 'manifest.json'} is not a {SCHEMA} manifest")
    return manifest


def log_event(out: Path, event: dict) -> None:
    with (out / "events.jsonl").open("a") as sink:
        sink.write(json.dumps({"at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), **event}) + "\n")
