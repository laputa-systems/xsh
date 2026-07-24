proc test_tar_create_list_extract(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "tar-src")?
  fp"${root}/file.txt".write("tar payload")?
  fp"${root}/other.txt".write("other payload")?
  let tarball = test.temp_path(ctx, name: "archive.tar")
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -cf $tarball -C $root . ?
  let listed = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -tf $tarball ?
  test.contains(listed, "file.txt")?
  let filtered = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -tf $tarball file.txt ?
  test.contains(filtered, "file.txt")?
  test.ok(! ("other.txt" in filtered))?
  let out = test.temp_dir(ctx, name: "tar-out")?
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -xf $tarball -C $out ?
  test.contains(fp"${out}/file.txt".read_text()?, "tar payload")?
  let selected = test.temp_dir(ctx, name: "tar-selected")?
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -xf $tarball -C $selected file.txt ?
  test.contains(fp"${selected}/file.txt".read_text()?, "tar payload")?
  test.ok(! fp"${selected}/other.txt".exists()?)?
  let err = test.temp_path(ctx, name: "tar.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -xf $tarball -C $out 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "destination exists")?
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- --overwrite -xf $tarball -C $out ?
}
