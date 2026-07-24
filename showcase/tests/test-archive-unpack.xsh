proc test_archive_unpack(ctx: TestContext) [fs, process, error] {
  let src = test.temp_dir(ctx, name: "arc-src")?
  fp"${src}/a.txt".write("alpha")?
  fp"${src}/b.txt".write("beta")?
  let tarball = test.temp_path(ctx, name: "test.tar.gz")
  run.text "tar" "czf" $tarball "-C" $src "." ?
  let out = test.temp_path(ctx, name: "arc-out")
  let extract_out = run.text "xsh" "showcase/archive-unpack.xsh" -- $tarball --out $out --dry-run=false ?
  test.contains(extract_out, "entries in test.tar.gz")?
  test.contains(extract_out, "extracted to")?
  test.ok(fp"${out}/a.txt".exists()?)?
  let usage = run.text "xsh" "showcase/archive-unpack.xsh" -- --help ?
  test.contains(usage, "usage:")?
}
