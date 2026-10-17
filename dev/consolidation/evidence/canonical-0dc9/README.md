# Failed canonical run on 0dc9cff7

The pinned canonical gate ran on source commit `0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4`, tree `b4f9db3f8ee2f38a6c62fea876976619db424ed7`. The receipt shows clean matching checks before and after the run.

The Linux x86_64 musl run used pinned image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`. Docker received 12 CPUs, 24 GiB memory, no swap, and no network. The runner executed `xsh dev/main.xsh -- internal gate-batch`, took 3,699 seconds, and exited 3. The native suite reported 13,906 passed, 9 failed, and 78 skipped. Peak cgroup memory was 5,092,859,904 bytes. The run recorded no OOM or swap events. The receipt grants no acceptance credit. No canonical acceptance or functional close was issued.

The nine failures are:

- `tests/xsh/tooling-test.xsh::test_runner_cov_api_opt_in_prints_api_sections` failed during discovery while scanning a mode-000 `.work/tmp` fixture.
- `tests/xsh/tooling-test.xsh::test_runner_cov_exact_prints_coverage_sections` failed during discovery with `Permission denied (os error 13)`.
- `tests/xsh/tooling-test.xsh::test_runner_cov_json_out_writes_structured_report` failed during discovery with `Permission denied (os error 13)`.
- `tests/xsh/tooling-test.xsh::test_runner_cov_list_does_not_execute_tests` failed during discovery with `Permission denied (os error 13)`. [Coverage discovery evidence](coverage-discovery.md) records the reproduced cause and source path for all four failures.
- `core/tests/test-gnu-rm.xsh::test_gnu_rm_ext3_perf_log` expected `rm -rf d` to exit 0 but received status 3 with empty stderr.
- `core/tests/test-uu-factor.xsh::test_uu_factor_parallel` timed out after 2 minutes and was canceled with signal 15.
- `core/tests/test-uu-sort-1.xsh::test_uu_sort_buffer_sizes` timed out after 4 minutes and was canceled with signal 15.
- `core/tests/test-gnu-mv.xsh::test_gnu_mv_leak_fd_log` timed out after 2 minutes and was canceled with signal 15.
- `core/tests/test-uu-tail-1.xsh::test_uu_tail_follow_when_files_are_pointing_to_same_relative_file_and_file_stays_same_size` received an extra `==> data <==` section containing `same data`.

The canonical runner does not establish causes for the last five failures. The full stdout and stderr retain their diagnostics and output.

## Product qualification

The pinned verification bootstrap passed on the same clean source. It used `cargo build --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --bin xsh --message-format=json-render-diagnostics`. The bootstrap `xsh` artifact was 31,552,464 bytes with SHA-256 `24136a2854e7b9c06dda7f2f008f169bb7eac36fa062660facd58aa771556a10`.

The canonical receipt records the products after the run. Its `xsh` is 31,645,688 bytes with SHA-256 `c137d1d1ccff63c3ae492af1106aebb1872ae23b3d0e2af1d017968a4aa19a6c`. Its `xsht` is 38,738,160 bytes with SHA-256 `5dccc9f21f30b4b26e8f96a7815303dd3b7a13c94b132d421ad02cee3fada7e1`. Its `xshi` is 587,320 bytes with SHA-256 `5121c56402853320e396695d6854336067fa9754fcc957bdcdd98ffe74fcea53`. These differ from the earlier bootstrap and are the post-run product identities.

The qualified product files remain in the ignored target at `.work/target-final-canonical/x86_64-unknown-linux-musl/verification/`. This evidence retains their source-bound receipt and hashes, not the ELF binaries. The separate default-product and compatibility-harness build records are also retained in this directory.

## Retained evidence

- [Canonical receipt](canonical-replay.json), [stdout](stdout.log), [stderr](stderr.log), and [container resource observation](container-resources.json) are byte-for-byte copies of the failed run. The receipt SHA-256 is `24ce002712b9104b924685acb753150735d764486318906908c44568ec00b130`.
- [Bootstrap receipt](bootstrap-receipt-0dc9cff7.json), [Cargo JSON output](bootstrap-cargo-0dc9cff7.jsonl), and [Cargo log](bootstrap-cargo-0dc9cff7.log) retain the source-bound verification bootstrap.
- [Default-product Cargo output](default-products-0dc9cff7.jsonl), [default-product log](default-products-0dc9cff7.log), and [compatibility-harness build log](harness-builds-0dc9cff7.log) retain the earlier product and harness qualification used to stage the immutable QEMU image.
