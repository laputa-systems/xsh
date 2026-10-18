from __future__ import annotations

from typing import Any


TARGET = "x86_64-unknown-linux-musl"
IMAGE = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"


def plan_data() -> dict[str, Any]:
    return {
        "plan_status": "approved",
        "source": {"binding": "execution requires the supplied clean commit and tree"},
        "environment": {
            "target": TARGET,
            "image": IMAGE,
            "laputa_corpus_binding": {
                "form": "--laputa-corpus PATH",
                "required": True,
                "must_be_clean_git_root": True,
            },
            "native_runner_base_path": "/home/josh/d/laputa-systems/xsh/.work/consolidation/infra/run-native.sh",
            "native_lock_path": "/home/josh/d/laputa-systems/xsh/.work/consolidation/infra/native-suite.lock",
            "native_cleanup_marker_path": "/home/josh/d/laputa-systems/xsh/.work/consolidation/infra/native-suite.cleanup-unresolved",
            "cleanup_command_timeout_seconds": 10,
            "runner_term_timeout_seconds": 5,
            "runner_kill_timeout_seconds": 5,
            "cleanup_verification": "docker ps --all must succeed and return no container matching the cidfile ID",
            "container_cpus": 12,
            "container_memory": "24g",
            "container_swap": "disabled",
            "cargo_lock_timeout_seconds": 900,
            "native_lock_scope": "acquired before product, harness, Clippy and supplemental context runs",
        },
        "products": [
            [
                "cargo", "build", "--locked", "--offline", "--profile", "verification",
                "--target", TARGET, "-p", "xsh", "-p", "xsht", "-p", "xshi", "-p", "xsh-fuzz",
                "--bins", "--message-format=json",
            ],
        ],
        "product_variants": {
            "verification": {
                "profile": "verification",
                "target_variant": "products",
                "binaries": ["xsh", "xsht", "xshi", "xsh-fuzz", "xsh-test-helper"],
            },
        },
        "rust_harness_builds": [
            {
                "id": "core-compat-boundaries",
                "profile": "verification",
                "args": ["-p", "xsh", "--test", "core_compat_boundaries"],
                "targets": ["core_compat_boundaries"],
                "target_variant": "harness",
            },
            {
                "id": "core-compat-extra",
                "profile": "verification",
                "args": ["-p", "xsh", "--test", "core_compat_extra"],
                "targets": ["core_compat_extra"],
                "target_variant": "harness",
            },
            {
                "id": "core-compat-extra-privileged",
                "profile": "verification",
                "args": [
                    "-p", "xsh", "--test", "core_compat_extra_privileged",
                    "--features", "linux-priv-tests",
                ],
                "targets": ["core_compat_extra_privileged"],
                "target_variant": "linux-priv",
            },
        ],
        "rust_harness_runs": [],
        "focused_native_files": [],
        "native_extra": [],
        "warning_deny": [
            [
                "cargo", "clippy", "--locked", "--offline", "--workspace", "--target", TARGET,
                "--all-targets", "--all-features", "--quiet", "--", "-D", "warnings",
            ],
        ],
        "linux_context_runs": [
            {
                "harness": "core-compat-boundaries",
                "filter": "",
                "skip_filter": "split::",
                "network": "none",
                "privileged": False,
                "memory": "4g",
            },
            {
                "harness": "core-compat-extra",
                "filter": "",
                "skip_filter": "split::",
                "network": "none",
                "privileged": False,
                "memory": "4g",
            },
            {
                "harness": "core-compat-extra",
                "filter": "split::",
                "network": "none",
                "privileged": False,
                "memory": "1g",
                "cpus": "0.01",
            },
            {
                "harness": "core-compat-extra-privileged",
                "filter": "cp::privileged::test_cp_recursive_block_device_",
                "network": "none",
                "privileged": True,
                "memory": "1g",
            },
            {
                "id": "traceroute-unprivileged",
                "binary": "xsht",
                "argv": ["test", "core/tests/test-traceroute.xsh", "-j", "1"],
                "network": "none",
                "privileged": False,
                "user": "1000:1000",
                "memory": "1g",
            },
        ],
        "launcher_self_tests": [
            {
                "path": "dev/consolidation/winddown/gate-tools/final-correctness-gates/test_launcher_cleanup.py",
            },
        ],
        "non_goals": [
            "No broad Rust or native test loops beyond the five isolated Linux contexts.",
            "The canonical batch owns the full native suite and docs gate.",
            "No performance claim is derived from this correctness gate.",
        ],
    }
