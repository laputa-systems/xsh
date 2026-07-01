pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_tar_create_list_extract(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "tar-src")?
  fp"${root}/file.txt".write("tar payload")?
  fp"${root}/other.txt".write("other payload")?
  let tarball = test.temp_path(ctx, name: "archive.tar")
  run.text xsh_bin() tar.xsh -- -cf $tarball -C $root . ?
  let listed = run.text xsh_bin() tar.xsh -- -tf $tarball ?
  test.contains(listed, "file.txt")?
  let filtered = run.text xsh_bin() tar.xsh -- -tf $tarball file.txt ?
  test.contains(filtered, "file.txt")?
  test.ok(! filtered.contains("other.txt"))?
  let out = test.temp_dir(ctx, name: "tar-out")?
  run.text xsh_bin() tar.xsh -- -xf $tarball -C $out ?
  test.contains(fp"${out}/file.txt".read_text()?, "tar payload")?
  let selected = test.temp_dir(ctx, name: "tar-selected")?
  run.text xsh_bin() tar.xsh -- -xf $tarball -C $selected file.txt ?
  test.contains(fp"${selected}/file.txt".read_text()?, "tar payload")?
  test.ok(! fp"${selected}/other.txt".exists()?)?
  let err = test.temp_path(ctx, name: "tar.err")
  let status = run.status xsh_bin() tar.xsh -- -xf $tarball -C $out 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "destination exists")?
  run.text xsh_bin() tar.xsh -- --overwrite -xf $tarball -C $out ?
}
