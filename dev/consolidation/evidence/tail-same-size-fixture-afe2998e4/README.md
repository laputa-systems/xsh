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

## Previous run qualification

The previously saved checker, selector, and three native outputs are preserved,
but they are not qualified evidence for this repair. The outer Docker invocation
mounted
`/home/josh/d/laputa-systems/xsh/.work/target-final-canonical/x86_64-unknown-linux-musl/verification`
at `/target:ro`, which is the main worktree's products directory. It did not
mount the designated products under the frozen `consolidation-final-0dc9cff7c`
worktree. The old outer argv was not saved alongside the inner
[container command](native-container-command.sh), so its product identity cannot
be reconstructed from that file. The recorded per-selector statuses and their
original wrapper status remain unchanged in [the old summary](native-summary.tsv)
and [status file](container-wrapper.exit-status.txt); no current-source credit
is assigned to them.

The qualified verification directory is
`/home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-0dc9cff7c/.work/target-final-canonical/x86_64-unknown-linux-musl/verification`.
Its products currently match these pins:

```text
c137d1d1ccff63c3ae492af1106aebb1872ae23b3d0e2af1d017968a4aa19a6c  xsh
5dccc9f21f30b4b26e8f96a7815303dd3b7a13c94b132d421ad02cee3fada7e1  xsht
```

A qualified three-selector replay is still required. Its outer Docker argv,
source pin, product hashes, raw per-selector outputs, and wrapper status will be
recorded with that replay.
