type Ran = {status: Int, stdout: Str, stderr: Str}

proc realpath_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/realpath.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), "--", @args]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_realpath { |ctx|
  let root = test.temp_dir(ctx, name: "realpath")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/realpath.xsh" -- $root ?
  assert output.trim() == root.resolve()?.display()
}

test test_realpath_dangling_links_and_trailing_slashes { |ctx|
  let root = test.temp_dir(ctx, name: "realpath-paths")?
  let file = fp"{root}/file"
  let link = fp"{root}/link"
  file.write("x")
  fs.symlink(file, link)
  let regular_slash = realpath_run(ctx, root, ["file/"])?
  assert regular_slash.status == 1
  let link_slash = realpath_run(ctx, root, ["link/"])?
  assert link_slash.status == 1
  let missing = realpath_run(ctx, root, ["-m", "missing/"])?
  assert missing.status == 0, missing.stderr
  assert missing.stdout.trim() == fp"{root}/missing".display()

  let dangling = fp"{root}/dangling"
  let destination = fp"{root}/not-created"
  fs.symlink(destination, dangling)
  let resolved = realpath_run(ctx, root, ["dangling"])?
  assert resolved.status == 0, resolved.stderr
  assert resolved.stdout.trim() == destination.display()

  let dangling_slash = realpath_run(ctx, root, ["dangling/"])?
  assert dangling_slash.status == 0, dangling_slash.stderr
  assert dangling_slash.stdout.trim() == destination.display()
}
