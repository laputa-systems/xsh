test test_tar_create_list_extract [fs, process, env, error] { |ctx|
  let root = test.temp_dir(ctx, name: "tar-src")?
  fp"${root}/file.txt".write("tar payload")?
  fp"${root}/other.txt".write("other payload")?
  let tarball = test.temp_path(ctx, name: "archive.tar")
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -cf $tarball -C $root . ?
  let listed = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -tf $tarball ?
  "file.txt" in listed
  let filtered = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -tf $tarball file.txt ?
  "file.txt" in filtered
  ! ("other.txt" in filtered)
  let out = test.temp_dir(ctx, name: "tar-out")?
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -xf $tarball -C $out ?
  "tar payload" in (fp"${out}/file.txt".read_text()?)
  let selected = test.temp_dir(ctx, name: "tar-selected")?
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -xf $tarball -C $selected file.txt ?
  "tar payload" in (fp"${selected}/file.txt".read_text()?)
  ! fp"${selected}/other.txt".exists()?
  let err = test.temp_path(ctx, name: "tar.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- -xf $tarball -C $out 2> $err
  ! status.exited_with(0)
  "destination exists" in (err.read_text()?)
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tar.xsh" -- --overwrite -xf $tarball -C $out ?
}
