proc test_release_pack(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "release-src")?
  let out = test.temp_path(ctx, name: "release-out")
  fp"${root}/bin".mkdir()?
  fp"${root}/bin/tool".write("tool")?
  let output = run.text "xsh" "showcase/release-pack.xsh" -- $root $out --dry-run=false ?
  test.contains(output, "archive ")?
  test.ok(fp"${out}/release.tar".exists()?)?
  test.ok(! fp"${out.parent}/.${out.name()}.xsh-stage".exists()?)?
}

proc test_release_pack_refuses_existing_output(ctx: TestContext) [fs, process, error] {
  let source = test.temp_dir(ctx, name: "release-source")?
  fp"${source}/input".write("new release")?
  let out = test.temp_dir(ctx, name: "existing-release")?
  let old_archive = fp"${out}/release.tar"
  old_archive.write("previous release")?

  let status = run.status "xsh" "showcase/release-pack.xsh" -- $source $out --dry-run=false
  test.ok(! status.exited_with(0), "existing output must not be replaced")?
  test.eq(old_archive.read_text()?, "previous release")?
}

proc test_release_pack_cleans_failed_staging(ctx: TestContext) [fs, process, error] {
  let source = test.temp_file(ctx, name: "not-a-directory", contents: b"invalid source")?
  let out = test.temp_path(ctx, name: "new-release")
  let pending = fp"${out.parent}/.${out.name()}.xsh-stage"
  let status = run.status "xsh" "showcase/release-pack.xsh" -- $source $out --dry-run=false
  test.ok(! status.exited_with(0), "invalid source must fail")?
  test.ok(! out.exists()?)?
  test.ok(! pending.exists()?)?
}

proc test_release_pack_rejects_output_inside_input(ctx: TestContext) [fs, process, error] {
  let source = test.temp_dir(ctx, name: "nested-source")?
  fp"${source}/input".write("unchanged")?
  let out = fp"${source}/release"
  let status = run.status "xsh" "showcase/release-pack.xsh" -- $source $out --dry-run=false
  test.ok(! status.exited_with(0), "output inside source would recurse during copy")?
  test.eq(fp"${source}/input".read_text()?, "unchanged")?
  test.ok(! out.exists()?)?
}
