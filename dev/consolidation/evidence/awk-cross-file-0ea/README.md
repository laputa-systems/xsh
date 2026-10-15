# Original 0ea AWK shard 001 discriminator

The AWK differential has a concrete cross-file filesystem cause. The original
0ea source and original-source release binaries reproduce the same shard-001
signature when the written curl test runs between the written and transformed
AWK runs. This identifies the mechanism behind the historical outcome; it
does not recreate the historical per-child output files that were not retained.

## Identity

Source is clean `0ea216732686b7851bfb8ba49b81b5ef6d2608b1`, tree
`cb8cd5298918f7a9839284d894acca4dd9a4e346`. Build used the pinned image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
Cargo release profile (thin LTO), locked offline dependencies, default features
(`xsh`: native-tests/net/tools; `xsht`: native-tests), Kache, and the Clang-23 /
mold musl linker. `TARGET=x86_64-unknown-linux-musl`; Rust flags were
`-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol`.
The exact build argv was `cargo build --locked --offline --release --target
x86_64-unknown-linux-musl -p xsh --bin xsh -p xsht --bin xsht` (exit 0).
Binary SHA-256: `xsh` `d34f67df729783ed82f05413b55be20a3ae14153c2109e1dcc3d979c39a58614`,
`xsht` `ef442764e3ad8de675fc76ee2873cdc100c31e0d015c99d86badc6bdf5997549`.

## Reproduction

The inner runner is `run-container.sh`; output and strace files are in
`runtime/`. Written and transformed XSH sources are retained as `.xsh.gz`. In one clean container it ran, in order:

1. `xsht test --jobs 8 --exact core/tests/test-awk.xsh::test_awk_matrix_streams_and_processes_shard_001` against written source: status 0.
2. `xsht test --jobs 8 --exact core/tests/test-curl.xsh::test_curl_write_failure_exits_23`: status 1; assertion reports expected 23, actual 0.
3. `xsht desugar core/tests/test-awk.xsh`, replace only the isolated AWK test source with that output, then rerun command 1: status 1, only `io_redirect_unwritable: stderr,status` differs.

For commands 1 and 3, strace shows case 11's exact child invocation
`xsh /replay/core/awk.xsh -- 'BEGIN { print "x" > "/nonexistent/dir/f" }'`.
The written child has empty stdout, expected fatal stderr
`awk: cmd. line:1: fatal: cannot redirect to \`/nonexistent/dir/f': No such file or directory\n`,
and status 2. The transformed child has empty stdout/stderr, writes `x\n` to
`/nonexistent/dir/f`, and exits 0. The matrix row is index 11 and pins the
expected result from GNU awk 5.3.2, default mode, UTF-8 locale (`regenerate.py`).

Initially all four `/nonexistent` paths were absent; they remained absent after
written AWK. After curl, `/nonexistent/dir` existed and `out` was six bytes.
After transformed AWK, `f` existed and was two bytes. Runtime was root,
`LC_ALL=C`, `TZ=UTC`, `XSH_MODULE_PATH` unset, `TMPDIR` on disk under `.work`,
jobs 8, cpuset 16–23, CPU quota 8, 16 GiB memory, no swap.

In 0ea, `test-curl.xsh:638` requests `/nonexistent/dir/out`. `core/curl.xsh`
only creates parents for `--create-dirs`, but `xsh-net::prepare_download_destination`
unconditionally calls `create_dir_all(parent)`. `xsht/tests/desugar.rs:202–238`
collects written reports for every selected file before desugaring files and
rerunning reports, so curl can create this path after written AWK and before
transformed AWK. The earlier 12-shard replay omitted the curl phase, so both
AWK-only runs passed shard 001; it did not test cross-file contamination.

The curl test's internal applet streams were not independently saved; its
retained outer stdout shows the status assertion and outer stderr is empty.
The historical child artifacts, exact environment, and exact worker count were
not retained. This discriminator uses the original source and original
binaries and reproduces the same case/signature in the harness's source-proven
order. It does not assert bytes for the missing historical streams or claim a
full 197-file replay. Full outputs, path snapshots, binary/source hashes, and
AWK strace evidence remain beside this note.
