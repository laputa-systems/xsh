# Tail same-size fixture diagnosis

The canonical run at source `0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4` failed
`test_uu_tail_follow_when_files_are_pointing_to_same_relative_file_and_file_stays_same_size`.
The [saved failure](baseline-failure.txt) shows `same data` a second time.

The fixture called `uu.write` while descriptor follow was active. `uu.write`
calls `Path.write`, and `src/modules/fs.rs::write_path_unnamed` opens the file
with `.truncate(true)` before writing. The follower can observe a size below its
read offset during that operation. `core/tail.xsh::follow_descriptors` treats
that as a real truncation, seeks to the start, and reads the new bytes. The
follower behaved correctly. The fixture did not perform the stable-size
overwrite named by the test.

Commit `afe2998e48045f8b2a44377fdd4507a78b6843ff` changes only the native test.
It writes the replacement bytes through a descriptor opened without truncate,
then checks the contents, mode, device, inode, and size after closing it. The
two existing truncation tests remain unchanged.

## Verification

The checks used the read-only verification products from source `0dc9cff7` in
the pinned image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
The container ran on `linux/amd64` as UID 1000 with two CPUs, 4 GiB memory, no
network, and the shared native-suite lock. `TMPDIR` pointed inside this lane's
ignored `.work/tmp` directory.

`xsht check core/tests/test-uu-tail-1.xsh` passed. The exact test-list command
selected one case:

```text
xsht test --list --exact core/tests/test-uu-tail-1.xsh::test_uu_tail_follow_when_files_are_pointing_to_same_relative_file_and_file_stays_same_size
```

Three exact native invocations each selected one test and returned status 0.
The corrected same-size case passed, as did both truncation controls. Their raw
outputs are [saved here](corrected-overwrite.stdout),
[here](uutils-truncation.stdout), and
[here](descriptor-truncation.stdout). [The selector and status rows](native-summary.tsv)
show all three results.

The outer shell wrapper returned status 1 after the native tests. Its final
`awk` predicate inverted the all-pass condition. Each native command returned
status 0, as recorded in `native-summary.tsv`. The [container command](native-container-command.sh)
preserves the predicate that returned 1. The recorded wrapper status is in
[this file](container-wrapper.exit-status.txt). [The corrected summary check](verify-summary.sh)
was run against the saved rows only. It did not rerun native tests.
