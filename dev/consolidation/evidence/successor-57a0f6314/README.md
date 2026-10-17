# C additive close and failed profile

Source: `57a0f63145a4ea3b1db6cfc38b4744a5c7ab02c1`, tree
`42d48a35f740fc99a525c11d8f8da7ff41ffb106`.

Root accepted the additive functional close recorded in
[functional-close.json](functional-close.json). Its SHA-256 is
`4b2e98771d605b7002175bb3473a9e9ac5ef79d0c42d2fba737eb7b2b68daa9a`. The
close inherits the unchanged 24cc scopes. It does not grant a fresh full
canonical or resource pass, timing acceptance, QEMU, W9, or a structural
recount.

## Integrated selectors

The integrated run passed each of these exact selectors once:

- `core/tests/test-sort.xsh::test_sort_key_option_suffixes_and_blank_boundaries`
- `core/tests/test-sort.xsh::test_sort_general_numeric_prepares_spilled_and_merge_keys`
- `core/tests/test-gnu-rm.xsh::test_gnu_rm_r_symlink_directory_log`

The preserved [stdout](integrated/stdout.log) has SHA-256
`3d831cbfd189a2ed023fa4e3774a82e6ebc8d1520c86756f9367c42824c0ffc0`.
Source, product, and clean-tree guards pass before and after the run. The
result is `command=0 cleanup=0 list=0 source=0 products=0 tree=0`; no owned
containers remain. The [container state](integrated/container-state.txt)
records `OOMKilled=false`, CPUs `28-31`, and equal 8 GiB memory and swap
limits, so the container had no swap allowance. The exact source guard,
product guards, cleanup outputs, CID, and result remain under `integrated/`.

The archived [launcher](integrated/run-next-integrated.sh) is the exact
`run-next-integrated.sh` copied from `.work/tmp`. The integrated scratch
`source.txt` and stdout bind it to source C and these three selectors.

## Failed normal profile

The [profile receipt](profile/receipt.json) records run
`01e8361f6668c79453920881e0b84ca7`, source C, and a clean source tree. The
release opt-level-3 build exited zero. Both `perf-gate-ready` checks exited
zero. Docker used CPUs `0-11`, 12 CPUs, 24 GiB of memory, and no swap. All
three product hashes match the prior 24cc profile products. The receipt marks
the profile failed and `timing_claim_eligible=false`.

The native suite exited 1 with 13,850 passes, four failures, and 141 skips.
The [native log](profile/opt3/native-suite.log) records these failures:

- `test_gnu_tail_debug_log` could not find `strace.out`.
- `test_tail_follow_name_switches_banners_between_files` grouped the two
  appended lines under different banners than expected.
- `timeout_stops_an_xsh_child_blocked_in_stdin_read` timed out after two
  seconds.
- `test_gnu_mv_leak_fd_log` timed out after two minutes and was canceled by
  signal 15.

The C run has 141 skips, two more than the 24cc R2 run. Comparing the actual
logs shows that the only added skips are
`core/tests/test-nc.xsh::test_nc_unix_datagram_listener_answers_a_client` and
`core/tests/test-nc.xsh::test_nc_unix_stream_client_and_listener`. Both passed
in R2. C skipped both because the temporary-directory socket path was too
long. The R2 receipt and native log are archived in the [24cc profile
record](../successor-24cc0d704/README.md#failed-timed-profile-attempts). This
comparison accounts for the skip-count change; it grants no new coverage
credit. C also selects one added symlink-referent regression, as recorded in
the close scope.

The old R2 `rm` and `sort` failures remain failed in their original receipt.
Their exact selectors pass in the focused repair records for [rm](../rm-cached-leaf-24cc/README.md)
and [sort](../sort-native-r2-b3e94ff/README.md). Those focused passes do not
establish the cause of the R2 failures. The C profile still failed on the four
cases above, so it earns no profile, timing, canonical, or resource credit.

The [build log](profile/opt3/build.log), [profile launch arguments](profile/launch/argv.txt),
launch stdout, stderr, and exit status are preserved with the receipt. Raw
files copied from the functional-close record, integrated scratch, profile
receipt, and launch directory were compared byte-for-byte with their sources.
