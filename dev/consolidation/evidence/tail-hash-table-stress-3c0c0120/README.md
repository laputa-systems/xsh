# Tail hash-table stress fixture repair

The canonical comparison failed because the stress fixture truncated a file
that tail was following. The fixture now preserves the file's inode and size
for same-size rewrites. Tail production code and the expected stream assertions
did not change.

## Cause and repair

At frozen source `3c0c01206db71443662242167ac5f0a4bfafc62b`, the stress test
reuses one scene for two rows. The first row writes `x\n` to file `9`. The
second row called `uu.write` with the same two bytes. `uu.write` calls
`Path.write`, and `src/modules/fs.rs::write_path_unnamed` opens with
`.truncate(true)` before writing. Tail could observe the same inode at size
zero and report `tail: 9: file truncated`. The [canonical excerpt](canonical-failure-excerpt.txt)
shows that warning before the expected rename messages. The written test passed
79 of 79 cases; the desugared test passed 78 and failed this one.

Commit `16abfbed3975da8dabb4e955fd182c99dabd22dd` keeps the initial creation
through `uu.write` and uses the existing `overwrite_in_place` helper when file
`9` already contains `x\n`. The helper writes through a descriptor without
truncation and checks device, inode, size, mode, and contents. The same-size
rewrite in `test_uu_tail_follow_name_truncate4` now uses that helper too. The
actual truncation cases in `test_uu_tail_follow_name_truncate2` and
`test_uu_tail_retry_descriptor_detects_truncation` remain unchanged.

The [one-shot baseline](baseline-once/summary.tsv) selected the stress test
once in each form, and both runs passed. That run did not reproduce the race.
Its wrapper returned 1 because the diagnostic script expected a recurrence.
The canonical failure remains the observed red result. No retry loop was used.

## Controlled zero-size observation

The [scratch probe](qualified-run/probe-source.xsh) was appended only to a
temporary copy of the test file. It first used `overwrite_in_place` while tail
followed the file. Device, inode, and size stayed
`66307:70280565:10`, and stderr stayed empty. It then opened the same file with
shell redirection and held the truncating descriptor before writing the same
ten bytes. Tail saw the same device and inode at size zero and emitted
`tail: file: file truncated`. The writer restored the original contents and
size. The [raw probe output](qualified-run/probe.stdout) records both states.

This controlled probe confirms how an observable zero-size window affects
tail. The frozen canonical failure confirms that the original synchronous
`Path.write` fixture exposed that window during a live follow.

## Qualified selectors

The fixed-source run selected each exact test once. All four passed in written
and desugared form:

| Selector | Written | Desugared |
|---|---:|---:|
| `test_uu_tail_follow_name_hash_table_stress` | 1 pass | 1 pass |
| `test_uu_tail_follow_name_truncate2` | 1 pass | 1 pass |
| `test_uu_tail_follow_name_truncate4` | 1 pass | 1 pass |
| `test_uu_tail_retry_descriptor_detects_truncation` | 1 pass | 1 pass |

The [selector table](qualified-run/summary.tsv) records each status and exact
selector. The corresponding raw stdout and stderr files are in
`qualified-run/`. Both `xsht desugar` commands returned zero. The Docker wrapper
returned zero.

The run used source commit `16abfbed3975da8dabb4e955fd182c99dabd22dd`, tree
`126ccda5c8b49f47cb20103d87d354f43308929a`, and test blobs
`89cf46b56cbfbb499910cba09a87b9b138b7adc5` and
`568c825245922db27fc9222bb91143254eb8926a`. The before and after source
snapshots match. No tail implementation, shared test helper, or filesystem
implementation changed.

The runner mounted the frozen products from
`/home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-3c0c01206/.work/target-final-canonical/x86_64-unknown-linux-musl/verification`
read-only at `/target`. Their SHA-256 hashes were:

```text
2576641bd019ea1ba46fa555e351668407dbbacabdf4d87cabcf63de32b5b017  xsh
5ba0907b1b5222f5dab0efa19496b711bb4036b14c0ab2c344d1d8987f7e8836  xsht
```

The [pre-run](qualified-run/products-before.sha256) and
[post-run](qualified-run/products-after.sha256) hashes match. In-container
hashes also match before and after. The run used image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`
on `linux/amd64`, UID 1000, two CPUs, 4 GiB memory, no swap, and no network.
The full outer Docker argv is in [docker.argv.txt](qualified-run/docker.argv.txt).
The [host runner](qualified-run/run.sh), [container script](qualified-run/container.sh),
and [probe source](qualified-run/probe-source.xsh) preserve the command path.

The full canonical suite was not rerun on this fixture-only commit.
