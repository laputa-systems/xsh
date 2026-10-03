test test_tar_create_list_extract { |ctx|
  let root = test.temp_dir(ctx, name: "tar-src")?
  fp"${root}/file.txt".write("tar payload")?
  fp"${root}/other.txt".write("other payload")?
  let tarball = test.temp_path(ctx, name: "archive.tar")
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -cf $tarball -C $root . ?
  let listed = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -tf $tarball ?
  assert "file.txt" in listed
  let filtered = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -tf $tarball file.txt ?
  assert "file.txt" in filtered
  assert ! ("other.txt" in filtered)
  let out = test.temp_dir(ctx, name: "tar-out")?
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -xf $tarball -C $out ?
  assert "tar payload" in fp"${out}/file.txt".read_text()?
  let selected = test.temp_dir(ctx, name: "tar-selected")?
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -xf $tarball -C $selected file.txt ?
  assert "tar payload" in fp"${selected}/file.txt".read_text()?
  assert ! fp"${selected}/other.txt".exists()?
  let err = test.temp_path(ctx, name: "tar.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -xf $tarball -C $out 2> $err
  assert ! status.exited_with(0)
  assert "destination exists" in err.read_text()?
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- --overwrite -xf $tarball -C $out ?
}
