pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_release_pack(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "release-src")?
  let out = test.temp_path(ctx, name: "release-out")
  fp"${root}/bin".mkdir()?
  fp"${root}/bin/tool".write("tool")?
  let output = run.text xsh_bin() "showcase/release-pack.xsh" -- $root $out --dry-run=false ?
  test.contains(output, "archive ")?
  test.ok(fp"${out}/release.tar".exists()?)?
}
