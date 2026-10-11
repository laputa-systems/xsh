#!/usr/bin/env python3
"""Generate dev/coreutils-parity.json from a pinned uutils/coreutils checkout.

The denominator is derived from the pinned upstream `Cargo.toml` feature sets,
never from a handwritten list. Utilities enter the manifest from
`feat_os_unix` (the normal Unix/Linux surface); SELinux-only utilities
(`feat_require_selinux`) are listed separately as capability-gated, and still
count toward the inventory total so the denominator never shrinks.

Test results are merged from the JSON reports written by the suite runners
(`dev/compat/results/*.json`); absent reports leave those fields null rather
than claiming a pass.

Usage:
    UUTILS_ROOT=../ref/uutils-coreutils python3 dev/compat/parity.py [--check]

`--check` exits non-zero when the checked-in manifest is stale. Without
`UUTILS_ROOT` it runs offline, which is what `make check` does: the utility
list, capability gates and upstream test counts come from the committed
manifest, and every XSH-side field (presence, aliases, native tests, suite
results, exclusions, gaps, and the `linux_surface` section built from
`surface.json`) is still recomputed and compared. Both modes also
fail when the totals differ from the denominator pinned in
`upstream.lock.json`, so the denominator cannot shrink without a deliberate
lock change.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CORE = REPO / "core"
COMPAT = REPO / "dev" / "compat"
LOCK = COMPAT / "upstream.lock.json"
ALIASES = CORE / "aliases.json"
SURFACE = COMPAT / "surface.json"
EXCLUSIONS = COMPAT / "exclusions.json"
RESULTS = COMPAT / "results"
MANIFEST = REPO / "dev" / "coreutils-parity.json"

ENTRY_SET = "feat_os_unix"
CAPABILITY_SETS = {"feat_require_selinux": "selinux"}


def expand(features: dict[str, list[str]], name: str, seen: set[str], utils_dir: Path) -> list[str]:
    """Expand a feature set into utility names (those with a src/uu crate), in first-seen order."""
    out: list[str] = []
    for item in features.get(name, []):
        if "/" in item:  # crate-feature toggles such as "cp/selinux"
            continue
        if item in seen:
            continue
        seen.add(item)
        if (utils_dir / item).is_dir():
            # Most utilities are optional dependencies; a few (e.g. `test`)
            # are features aliasing `uu_<name>`. Either way the crate decides.
            out.append(item)
        elif item in features:
            out.extend(expand(features, item, seen, utils_dir))
    return out


def upstream_utilities(root: Path) -> tuple[list[str], dict[str, str]]:
    cargo = tomllib.loads((root / "Cargo.toml").read_text())
    features = cargo["features"]
    # Only names with a src/uu/<name> crate are utilities; this filters out
    # helper features such as "selinux" or "expensive_tests".
    utils_dir = root / "src" / "uu"
    base = expand(features, ENTRY_SET, set(), utils_dir)
    gated: dict[str, str] = {}
    for feature_set, capability in CAPABILITY_SETS.items():
        for util in expand(features, feature_set, set(), utils_dir):
            if util not in base:
                gated[util] = capability
    return base, gated


def upstream_tests(root: Path) -> dict[str, int]:
    """Count #[test] functions per utility in tests/by-util."""
    counts: dict[str, int] = {}
    for path in sorted((root / "tests" / "by-util").glob("test_*.rs")):
        util = path.stem.removeprefix("test_")
        counts[util] = len(re.findall(r"#\[test\]", path.read_text(errors="replace")))
    return counts


def load_json(path: Path, default):
    if path.exists():
        return json.loads(path.read_text())
    return default


def xsh_implementation(util: str, aliases: dict[str, str]) -> str | None:
    direct = CORE / f"{util}.xsh"
    if direct.exists():
        return f"core/{util}.xsh"
    target = aliases.get(util)
    if target and (CORE / f"{target}.xsh").exists():
        return f"core/{target}.xsh"
    return None


def native_tests(util: str, impl: str | None) -> str | None:
    test = CORE / "tests" / f"test-{util}.xsh"
    if test.exists():
        return f"core/tests/test-{util}.xsh"
    if impl:
        base = Path(impl).stem
        shared = CORE / "tests" / f"test-{base}.xsh"
        if shared.exists():
            return f"core/tests/test-{base}.xsh"
    # A family test file (test-procps, test-compress, test-storage) covers its
    # commands by name; take the file that mentions the command most, with at
    # least two uses so a passing mention in an unrelated test does not count.
    word = re.compile(r"(?<![\w-])" + re.escape(util) + r"(?![\w-])")
    best: tuple[int, str] | None = None
    for path in sorted((CORE / "tests").glob("test-*.xsh")):
        hits = len(word.findall(path.read_text(errors="replace")))
        if hits >= 2 and (best is None or hits > best[0]):
            best = (hits, path.name)
    return f"core/tests/{best[1]}" if best else None


def suite_summary(report: dict, util: str) -> dict | None:
    entry = report.get("utilities", {}).get(util)
    if entry is None:
        return None
    return {k: entry.get(k, 0) for k in ("pass", "fail", "skip", "excluded")}


def upstream_from_manifest(manifest: dict) -> tuple[list[str], dict[str, str], dict[str, int]]:
    rows = manifest.get("utilities", [])
    base = [r["utility"] for r in rows if r["capability"] is None]
    gated = {r["utility"]: r["capability"] for r in rows if r["capability"] is not None}
    counts = {r["utility"]: r["uutils_test_count"] for r in rows}
    return base, gated, counts


def surface_rows(aliases: dict[str, str]) -> list[dict]:
    """Per-command status for the expanded scope in surface.json (no upstream tree needed)."""
    rows = []
    for entry in load_json(SURFACE, {"commands": []})["commands"]:
        impl = xsh_implementation(entry["command"], aliases)
        rows.append(
            {
                "command": entry["command"],
                "phase": entry["phase"],
                "domain": entry["domain"],
                "reference": entry["reference"],
                "optional": entry["optional"],
                "xsh_implementation": impl,
                "present": impl is not None,
                "native_tests": native_tests(entry["command"], impl),
            }
        )
    return rows


def build(root: Path | None, committed: dict | None = None) -> dict:
    lock = load_json(LOCK, {})
    aliases = {e["name"]: e["target"] for e in load_json(ALIASES, {}).get("aliases", [])}
    exclusions = load_json(EXCLUSIONS, {"tests": []})
    gaps = load_json(COMPAT / "gaps.json", {}).get("utilities", {})
    uu_tests = load_json(RESULTS / "uutils-integration.json", {})
    gnu = load_json(RESULTS / "gnu-differential.json", {})
    busybox = load_json(RESULTS / "busybox.json", {})

    if root is None:
        base, gated, test_counts = upstream_from_manifest(committed or {})
    else:
        base, gated = upstream_utilities(root)
        test_counts = upstream_tests(root)
    excluded_by_util: dict[str, list[str]] = {}
    for item in exclusions.get("tests", []):
        excluded_by_util.setdefault(item["utility"], []).append(item["id"])

    rows = []
    for util in base + sorted(gated):
        impl = xsh_implementation(util, aliases)
        rows.append(
            {
                "utility": util,
                "capability": gated.get(util),
                "xsh_implementation": impl,
                "present": impl is not None,
                "native_tests": native_tests(util, impl),
                "uutils_test_count": test_counts.get(util, 0),
                "uutils_integration": suite_summary(uu_tests, util),
                "gnu_differential": suite_summary(gnu, util),
                "busybox": suite_summary(busybox, util),
                "excluded_tests": sorted(excluded_by_util.get(util, [])),
                "known_semantic_gaps": gaps.get(util, []),
            }
        )

    present = sum(r["present"] for r in rows)
    in_scope = [r for r in rows if r["capability"] is None]
    return {
        "generated_by": "dev/compat/parity.py",
        "upstream": {
            "repository": lock.get("uutils", {}).get("repository"),
            "commit": lock.get("uutils", {}).get("commit"),
            "entry_feature_set": ENTRY_SET,
        },
        "totals": {
            "upstream_utilities": len(rows),
            "in_scope": len(in_scope),
            "capability_gated": len(rows) - len(in_scope),
            "present": present,
            "present_in_scope": sum(r["present"] for r in in_scope),
            "missing_in_scope": sorted(r["utility"] for r in in_scope if not r["present"]),
        },
        "utilities": rows,
        "linux_surface": surface_summary(surface_rows(aliases)),
    }


def surface_summary(rows: list[dict]) -> dict:
    return {
        "totals": {
            "commands": len(rows),
            "present": sum(r["present"] for r in rows),
            "optional": sum(r["optional"] for r in rows),
            "missing": sorted(r["command"] for r in rows if not r["present"]),
        },
        "commands": rows,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--uutils-root", default=os.environ.get("UUTILS_ROOT"))
    args = parser.parse_args()
    if not args.uutils_root and not args.check:
        print("set UUTILS_ROOT or pass --uutils-root", file=sys.stderr)
        return 2
    root = Path(args.uutils_root).resolve() if args.uutils_root else None

    lock = load_json(LOCK, {})
    want = lock.get("uutils", {}).get("commit")
    if root is not None:
        head = (root / ".git" / "HEAD").read_text().strip() if (root / ".git").exists() else None
        if head and head.startswith("ref:"):
            ref = root / ".git" / head.split()[1]
            head = ref.read_text().strip() if ref.exists() else None
        if want and head and head != want:
            print(f"UUTILS_ROOT is at {head}, lock pins {want}", file=sys.stderr)
            return 2

    committed = load_json(MANIFEST, None)
    if root is None and committed is None:
        print(f"{MANIFEST.relative_to(REPO)} is missing; run parity.py with UUTILS_ROOT set", file=sys.stderr)
        return 1
    manifest = build(root, committed)
    pinned = lock.get("denominator")
    if pinned:
        totals = dict(manifest["totals"])
        totals["surface_commands"] = manifest["linux_surface"]["totals"]["commands"]
        for key, value in pinned.items():
            if key == "surface_commands":
                # Additions are scope growth and raise the pin deliberately; a
                # drop below the pin is a shrunken denominator.
                if totals[key] < value:
                    print(f"denominator shrank: surface_commands is {totals[key]}, lock pins {value}", file=sys.stderr)
                    return 1
            elif totals[key] != value:
                print(f"denominator shrank or grew: {key} is {totals[key]}, lock pins {value}", file=sys.stderr)
                return 1
    text = json.dumps(manifest, indent=2) + "\n"
    if args.check:
        if not MANIFEST.exists() or MANIFEST.read_text() != text:
            print(f"{MANIFEST.relative_to(REPO)} is stale; rerun dev/compat/parity.py", file=sys.stderr)
            return 1
        return 0
    MANIFEST.write_text(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
