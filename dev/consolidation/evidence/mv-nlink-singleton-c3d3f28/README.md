# Singleton cross-device moves

The frozen C profile timed out in `test_gnu_mv_leak_fd_log`. That unchanged test
moves 3,906 directories, each with one regular file, across filesystems. The
native harness keeps its 120-second default.

`copy_move_node` searched every retained file identity before copying a file.
`copy_across_devices` then revisited the full list to publish staged paths. The
change keeps those identities only when `nlink != 1`. It performs the existing
identity lookup first, so a later source alias with `nlink == 1` can still link
to an earlier copied peer. Other link counts keep the previous append behavior.

## Source-derived visit count

For the fixture's distinct single-link files, the old identity search visited
`0 + ... + 3,905 = 7,626,465` records. The path-publication loop visited
`1 + ... + 3,906 = 7,630,371` records. The two loops visited 15,256,836 records
in total. With the change, each source file has `nlink == 1`, so the retained
list stays empty and both loops visit zero records. These counts follow from the
source loops and fixture; they are not timing results.

## Native checks

Four existing hardlink controls passed: cross-device directory peers,
cross-operand peers, nested three-link groups, and multiple hardlink groups
with a singleton file. The unchanged `test_gnu_mv_leak_fd_log` selector also
passed. Each selector reported one pass, zero failures, and zero skips. The
selectors are listed in [selectors.txt](selectors.txt). The tests used the
default harness timeout and unchanged fixtures. Each container ran
`/tools/xsht test --jobs 1 --exact SELECTOR` from the read-only worktree.

Runs used the pinned Linux image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
CPUs 28–31, 8 GiB memory with no extra swap, and native lock inode `62012470`.
The worktree and protected C `xsh`, `xsht`, and `xsh-test-helper` products were
read-only in the container. All five containers exited with status 0 and
reported `OOMKilled=false`.

The candidate source was tested from the private worktree based on
`57a0f63145a4ea3b1db6cfc38b4744a5c7ab02c1`. `core/mv.xsh` had SHA-256
`c3d3f28c57aea12ac11fc86caab99ab7f29918968102676099d70e3ece7bb278`. The
unchanged `core/tests/test-gnu-mv.xsh` had SHA-256
`6101ba13b19994134b6cbceb413f32300b2db5a050b240fd4c011549e1b9e1c8`. Both
source hashes and all three product hashes matched before and after every
selector. The raw outputs are in [native-outputs.txt](native-outputs.txt), the
per-run guards are in [hash-guards.txt](hash-guards.txt). The raw launcher
command remains in the private `.work/tmp` run directory for proof review.

No timing comparison is claimed. No source-level deadline, fixture size,
default, or job cap changed.
