# Resources-only observer attempts for 7eb62cd9

## Source and controller

The observer receipts bind source commit `7eb62cd993e578c13bf6befd68a6a6ac15133c78`, tree `736b4bc16b1bcf07a0250b97e849506c19fb0753`. The functional-close record has SHA-256 `349b90176125d95b2c6a550d079f50918680b0ddde251cfcf918f9e6cb594da4`.

The installed observer controller is commit `ad59f0c6d9554f7e6891e914e5d9e976f841da81`. Its profile qualifier is commit `b55c2f379563c2bc68da17ea7ca13d8977877b5f`, with SHA-256 `39948e7c9f6ca85fb06c33ee4fab8a777e53b1f4b40733ac66776a8473739556`. The six installed files, manifest body, and sidecar are in `observer-pin/`. The manifest body SHA-256 is `de31460e1232ddfcb118f314e59d27109ca204758ba74e9952129576fa1d27f5`.

R3 used run ID `6de5734cb16c1f2aca115ee6663d3125` and output directory `resources-7eb62cd9-20261017-r3`. The receipt records the exact binary paths, sizes, and hashes. This archive contains no targets or built binaries.

## Attempt results

`attempts/path-preflight/` preserves the first launcher result. It exited 1 before starting the observer because `cargo` was missing from `PATH`. Its stdout was empty.

`attempts/resources-r1/` preserves the first observer run after the PATH correction. The isolated lint stage completed. The canonical batch was stopped after its Docker command lacked the agreed CPU and memory limits. No full native suite started. Its original receipt records unresolved container cleanup.

`attempts/resources-r2/` preserves the second observer run. The canonical batch deadlocked when the observer held Cargo's lock while its child needed the same lock. The run was interrupted before the full native suite. Its receipt records resolved cleanup.

`attempts/resources-r3/` preserves the final observer run. Its canonical batch started one full native suite and failed with 13,914 passes, 2 failures, and 78 skips. `dev/tests/test-lifecycle.xsh::test_docker_container_failure_runs_target_ownership_cleanup` expected a verification-profile build command but observed a release-profile command. `core/tests/test-uu-factor.xsh::test_uu_factor_parallel` timed out after two minutes. The batch exited 3, and the observer receipt state is `failed`.

This observer sequence contributes one started full native suite to the ten-suite campaign count. R1 and R2 started none. The artifacts-only profile build added none. R3 does not pass the final resource gate.

## R3 resource observations

R3 ran in `resources-only` mode. Both outer and nested `wall_ns` fields are null, and both receipts mark timing as discarded. The run provides no timing result or timing acceptance.

The pinned container image was `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`. The canonical batch had a 12-CPU quota, cpuset `0-11`, 24 GiB memory limit, and zero swap. Its 100 ms sampler recorded 61,788 samples. The cgroup memory peak was 5,420,113,920 bytes; visible-process RSS peaked at 4,569,407,488 bytes. OOM and OOM-kill counts were zero.

The cgroup bounds cover the Docker batch. The host-side XSH bootstrap runs before that container and is outside those bounds. The outer observer rejected the batch because the canonical command failed, so these raw measurements do not earn final resource-gate credit.

## Cleanup records

R1's receipt remains unchanged and retains its original unresolved cleanup result. `cleanup-reconciliation/` contains the original lock and marker observations plus the root-owned follow-up cleanup log. The follow-up resolved the R1 container and lock ownership without changing the receipt. R2 and R3 cleanup results remain in their original receipts.

All attempt receipts, stage stdout and stderr, resource captures, Docker command logs, installed pin files, and reconciliation logs are copied byte-for-byte from their original paths. The `path-preflight` and `launcher` directories preserve host stdout, stderr, and exit codes separately from the observer receipts. No target directories, profile artifact directories, or binaries are included.
