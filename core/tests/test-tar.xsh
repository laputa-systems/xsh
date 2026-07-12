proc xsh_bin() [env] -> Path {
  let bin = (env.get("CARGO_BIN_EXE_xsh") ?? "")
  if bin != "" {
    return fp"${bin}"
  }
  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = (env.get("XSH_CORE_DIR") ?? "")
  if dir != "" {
    return fp"${dir}/${name}"
  }
  return ../name
}

proc test_tar_create_list_extract(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "tar-src")?
  fp"${root}/file.txt".write("tar payload")?
  fp"${root}/other.txt".write("other payload")?
  let tarball = test.temp_path(ctx, name: "archive.tar")
  run.text xsh_bin() core_script("tar.xsh") -- -cf $tarball -C $root . ?
  let listed = run.text xsh_bin() core_script("tar.xsh") -- -tf $tarball ?
  test.contains(listed, "file.txt")?
  let filtered = run.text xsh_bin() core_script("tar.xsh") -- -tf $tarball file.txt ?
  test.contains(filtered, "file.txt")?
  test.ok(! ("other.txt" in filtered))?
  let out = test.temp_dir(ctx, name: "tar-out")?
  run.text xsh_bin() core_script("tar.xsh") -- -xf $tarball -C $out ?
  test.contains(fp"${out}/file.txt".read_text()?, "tar payload")?
  let selected = test.temp_dir(ctx, name: "tar-selected")?
  run.text xsh_bin() core_script("tar.xsh") -- -xf $tarball -C $selected file.txt ?
  test.contains(fp"${selected}/file.txt".read_text()?, "tar payload")?
  test.ok(! fp"${selected}/other.txt".exists()?)?
  let err = test.temp_path(ctx, name: "tar.err")
  let status = run.status xsh_bin() core_script("tar.xsh") -- -xf $tarball -C $out 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "destination exists")?
  run.text xsh_bin() core_script("tar.xsh") -- --overwrite -xf $tarball -C $out ?
}
