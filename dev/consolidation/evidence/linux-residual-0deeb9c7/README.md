# Linux residual native qualification

## Scope and pins

This evidence covers the 36 exact selectors in [the standard list](standard-selectors.txt) and [the namespace selector](sysadmin-selector.txt). It uses frozen source commit `7eb62cd993e578c13bf6befd68a6a6ac15133c78`, tree `736b4bc16b1bcf07a0250b97e849506c19fb0753`, and the R6-qualified product pair at:

`/home/josh/d/laputa-systems/xsh/.work/final-review-evidence/final-correctness-gates/correctness-7eb62cd99-20261017-r1/target/products/x86_64-unknown-linux-musl/verification`

The qualified `xsh` SHA-256 is `d817ed7e8e43e5b7db5670068eb6fe76186178416233360aa535a5d07b5917af`. The qualified `xsht` SHA-256 is `4e0322037add1138f0e3a67f6696e3b27e7ae60ea794f198e0c5ea0d6bae8a6e`.

[Reproduction commands](COMMANDS.md) use the captured runners and these pins.

The runner used pinned image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`, `linux/amd64`, no network, 4 CPUs, 8 GiB memory, and no swap. It mounted source and products read-only, the owned scratch directory read-write, and the common Git directory read-only at its original host path. It ran each selector with `xsht test --jobs 1 --exact` and the existing 120-second test timeout.

Both contexts ran as container uid and gid 0 with supplementary groups 123 and 456. The standard context added CHOWN, SETUID, SETGID, SETPCAP, SYS_CHROOT, and NET_RAW, and dropped SYS_ADMIN. The single namespace selector used the same context with SYS_ADMIN added. Docker isolated the network and PID namespaces from the host.

## Outcomes

| Attempt | Context | Selected | Passed | Failed | Skipped | Result |
|---|---|---:|---:|---:|---:|---|
| Startup guard | standard | 0 | 0 | 0 | 0 | Git rejected `/repo` as dubious ownership before selector execution. Docker exited 2. |
| Startup guard | namespace | 0 | 0 | 0 | 0 | Same Git guard failure before selector execution. Docker exited 2. |
| Full selection | standard | 35 | 34 | 0 | 1 | The `ss` selector skipped because its generated socket path exceeded 100 bytes. Docker exited 1 under the zero-skip rule. |
| Full selection | namespace | 1 | 1 | 0 | 0 | Passed. Docker exited 0. |
| Short-path follow-up | standard | 1 | 1 | 0 | 0 | The exact `ss` selector passed with `TMPDIR=/t`. Docker exited 0. |

The startup-guard logs are in [attempt 1](attempt-1-git-guard/). They record both container IDs and confirm that cleanup removed both containers. The Git environment in later attempts trusts only the read-only `/repo` mount through `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=safe.directory`, and `GIT_CONFIG_VALUE_0=/repo`.

The full-selection logs and per-selector summaries are in [attempt 2](attempt-2-full-selection/). They preserve 34 ordinary passes, the single path-length skip, and the passing namespace selector. The two traceroute selectors passed with NET_RAW and network isolation unchanged.

## Traceroute coverage

`docs/TESTING.md` says the privileged root full-native batch covers positive loopback source-route traceroute. The active `gates.batch` to `tests.native` path drops Linux native runners to `HOST_UID` and `HOST_GID` with `setpriv`. The R6 privileged context selects a Rust `cp` test, so it does not execute these XSH tests. This residual therefore includes `core/tests/test-traceroute.xsh::test_traceroute_gateway_adds_a_source_route` and `core/tests/test-traceroute.xsh::test_traceroute_tcp_syn_reaches_open_and_closed_ports`. Both passed with NET_RAW, `--network=none`, and loopback-only traffic.

The skip came from `core/tests/test-ss.xsh::test_ss_unix_sockets_print_inodes_as_ports`. Its source skips when the generated Unix socket pathname exceeds 100 bytes. The first short scratch prefix, `/w/.work/tmp/t`, still exceeded that limit. [Attempt 3](attempt-3-ss-short-path/) reran only this selector with the same standard capabilities and an additional bind mount from the same owned scratch subdirectory to `/t`. The test's built-in length guard did not skip, and the socket assertions passed. The selector continues only when the displayed path is at most 100 bytes. The owned `/t` scratch was empty after the container exited.

Across the successful attempts, all 36 unique selectors passed at least once: 34 standard selectors in attempt 2, the `ss` selector in attempt 3, and the namespace selector in attempt 2. There were 37 selector executions, including the one retained and resolved skip from attempt 2. The full-selection attempt retains its one skip in the raw results.

## Guard and cleanup records

The full-selection and short-path logs record source commit, tree, clean status, and both product SHA-256 values before execution, inside each container, and after execution. Every recorded value matched the pins above. The startup-guard logs record matching host source and product guards before and after execution. They also record matching product hashes inside each container before Git rejected `/repo`. Each owned container ID reached `container_terminal=absent` with `cleanup_exit=0`. The exact Docker arguments, including capabilities, mounts, and resource limits, appear in each context log. The per-attempt runner scripts are included with the raw output.
