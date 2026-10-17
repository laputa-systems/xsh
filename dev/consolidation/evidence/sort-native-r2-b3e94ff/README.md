# Sort buffer timeout repair evidence

This record covers the `core/sort.xsh` repair for the R2 buffer-size timeout. It does not claim acceptance of the full R2 profile.

## Cause and change

The normal opt3 R2 native profile at base `24cc0d704209a8c0aa0eb27d5cfea1434929dc20` timed out in `core/tests/test-uu-sort-1.xsh::test_uu_sort_buffer_sizes` after four minutes. The test sorts 20,000 rows with nine buffer-size forms. The preserved trace counted 200,012 calls to `selected_sort_mode` for that workload.

The repair stores invariant sort-mode and raw-byte tie-break decisions in a per-batch record plan. Per-key options still determine key-specific modes. Numeric prefix normalization now returns the parsed prefix directly when `decimal_point == "."` and `thousands_separator.len() != 1`. Under that condition, `numeric_prefix_raw` accepts only an optional minus, ASCII digits, and the dot decimal point. Other locale settings retain the existing normalization path.

The source commit is `b3e94ff419e7e619908626ed8765f569f9a95754`. Its `core/sort.xsh` SHA-256 is `1363e18bc6c5c61de7e13fffc18ce00ec1e39b050c5995d42d0cebda9b161ae7`. The same source bytes were used for the pre-commit focused selectors and buffer-size selector. The four full sort files ran on the clean source commit.

## Bounded CPU comparisons

Both comparisons used 4,000 numeric rows, `-n -S 10T`, the protected `xsh` product, and five retained interleaved baseline/candidate pairs. Each run used `/usr/bin/time` CPU time. `perf-gate-ready` returned zero before and immediately after every retained run. All baseline and candidate outputs had SHA-256 `99047f346b7467c69a32f7b81a6cfd2cfb045e62a7852ab4a827e37eede84cbd`.

| Change | Baseline source | Candidate source | Baseline median CPU | Candidate median CPU | Median change |
|---|---|---|---:|---:|---:|
| Mode-plan hoisting | `9795751bd3ac62ff1a085ab78a576afe7790b0a2c7cefd6db30247bef3c038f4` | `a2d14d2cee5dbb714720a4bef82c0d53b4ca65b99c545ab554a8996ea1982a1f` | 2.67 s | 1.86 s | -30.3% |
| Prefix normalization fast path | `a2d14d2cee5dbb714720a4bef82c0d53b4ca65b99c545ab554a8996ea1982a1f` | `1363e18bc6c5c61de7e13fffc18ce00ec1e39b050c5995d42d0cebda9b161ae7` | 1.82 s | 1.46 s | -19.8% |

The mode-plan record contains six attempted pairs. Pair 4 is excluded because progress inspection overlapped its baseline sample. The five retained mode-plan pairs are 1, 2, 3, 5, and 6. All five normalization pairs are retained. These 4,000-row results do not predict the full 20,000-row profile.

## Correctness results

The 25 focused selectors passed with zero failures or skips on source SHA `1363e18bc6c5c61de7e13fffc18ce00ec1e39b050c5995d42d0cebda9b161ae7`. The unchanged buffer-size selector then passed once in 1 minute 35 seconds, with zero failures or skips, on the same source bytes and protected products. Its original 240-second test budget was unchanged. That single duration is a functional result, not a timing comparison.

The following four files ran serially on source commit `b3e94ff419e7e619908626ed8765f569f9a95754`:

| File | Passed | Failed | Skipped |
|---|---:|---:|---:|
| `core/tests/test-uu-sort-1.xsh` | 81 | 0 | 0 |
| `core/tests/test-uu-sort-2.xsh` | 79 | 0 | 4 |
| `core/tests/test-uu-sort-3.xsh` | 23 | 0 | 0 |
| `core/tests/test-sort.xsh` | 96 | 0 | 0 |
| Total | 279 | 0 | 4 |

The four skips are French, Hungarian, and Japanese UTF-8 locale cases. The French locale has two skipped cases. The locale was unavailable in the pinned image.

## Run conditions and limits

Runs used image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c` and the protected products. Their hashes are recorded in the raw archive. Native test containers used four CPUs on CPU IDs 24–27, 8 GiB memory, no extra swap, no network, and UID 1000. They used the original native lock inode `62012470`. No Rust build or full suite ran in this lane.

The selector and file runners recorded configured limits, product hashes, source hashes, exit statuses, and cleanup status. They did not retain post-run cgroup or OOM counters. This record does not include an R2 full-profile rerun. The final profile gate is outside this lane.

[`raw.tar.gz`](raw.tar.gz) contains the text source snapshots, 4,000-row input and output, CPU and readiness records, runner commands, 25 selector outputs and statuses, the unchanged selector output and guards, all four file outputs and guards, and the original trace excerpt. It contains no binaries or build cache. Source snapshots and runner commands are stored inside the archive as `.txt` files.
