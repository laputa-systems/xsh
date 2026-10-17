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

## Qualified verification

The corrected three-selector replay used
[the outer runner](qualified-run.sh) and [container command](qualified-container-command.sh).
The [captured Docker argv](qualified-run/docker.argv.txt) mounts the lane source
read-only and the designated frozen products read-only at `/target`. The image
identity is recorded in [image-identity.txt](qualified-run/image-identity.txt):
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`
on `linux/amd64`, UID 1000, two CPUs, 4 GiB memory with no swap, and no network.

The replay source was HEAD `57963af5377dd817396485bdb983416259f06e79`, tree
`cd87b3d2bcc6538346e76e8fbdfc1e8901e42116`, based on canonical source
`0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4`. The fixture blob was
`69bd9994fc81a5f1822b9da28636a5677658f1ce`; `core/tail.xsh`, the shared write
helper, and `Path.write` implementation matched their canonical base blobs.
The [before](qualified-run/source-before.txt) and
[after](qualified-run/source-after.txt) source snapshots match.

The designated products were the binaries under
`/home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-0dc9cff7c/.work/target-final-canonical/x86_64-unknown-linux-musl/verification`.
The [pre-run](qualified-run/products-before.sha256) and
[post-run](qualified-run/products-after.sha256) host hashes both match the
qualified xsh and xsht pins above; the in-container hashes also match before
and after ([before](qualified-run/products-container-before.sha256),
[after](qualified-run/products-container-after.sha256)).

All three exact selectors selected one test and returned status 0. The same-size
case and both existing truncation controls passed. [Selector counts and exit
statuses](qualified-run/summary.tsv) and [selection counts](qualified-run/selection-counts.tsv),
plus each raw output, are saved in `qualified-run/`: [same-size stdout](qualified-run/01.stdout),
[uutils truncation stdout](qualified-run/02.stdout), and
[descriptor truncation stdout](qualified-run/03.stdout). The Docker wrapper
exited 0, recorded in [docker.exit-status.txt](qualified-run/docker.exit-status.txt).
