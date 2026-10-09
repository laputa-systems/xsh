type Ran = {status: Int, stdout: Str, stderr: Str}

proc chgrp_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "chgrp-run")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/chgrp.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_chgrp_current_group { |ctx|
  let target = test.temp_file(ctx, name: "grouped.txt", contents: b"payload")?
  let current = group.current()?
  let name = current.name
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chgrp.xsh" $name $target ?
  assert output == ""
  assert target.metadata()?.gid == current.gid
  let root = test.temp_dir(ctx, name: "grouped-tree")?
  let child = fp"{root}/child.txt"
  child.write("payload")
  let recursive = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chgrp.xsh" -R $name $root ?
  assert recursive == ""
  assert child.metadata()?.gid == current.gid
}

test test_chgrp_reference_from_filter_and_verbose { |ctx|
  let root = test.temp_dir(ctx, name: "chgrp-modes")?
  let target = fp"{root}/target"
  let reference = fp"{root}/reference"
  target.write("target")
  reference.write("reference")
  let current = group.current()?
  let current_user = user.current()?
  fs.chgrp(target, current)

  let filtered = chgrp_run(ctx, ["--from", f"{current_user.uid}:{current.gid + 1}", "0", target.display()])?
  assert filtered.status == 0, filtered.stderr
  assert fs.stat(target)?.gid == current.gid

  let retained = chgrp_run(ctx, ["-v", f"{current.gid}", target.display()])?
  assert retained.status == 0, retained.stderr
  assert "retained as" in retained.stdout, retained.stdout

  let copied = chgrp_run(ctx, ["--reference", reference.display(), target.display()])?
  assert copied.status == 0, copied.stderr
  assert fs.stat(target)?.gid == fs.stat(reference)?.gid
}

test test_chgrp_reports_invalid_group_and_suppresses_quiet_errors { |ctx|
  let target = test.temp_file(ctx, name: "chgrp-invalid", contents: b"x")?
  let invalid = chgrp_run(ctx, ["__xsh_no_such_group__", target.display()])?
  assert invalid.status == 1
  assert invalid.stderr == "chgrp: invalid group: '__xsh_no_such_group__'\n", invalid.stderr

  let quiet = chgrp_run(ctx, ["-f", "0", fp"{target}.missing".display()])?
  assert quiet.status == 1
  assert quiet.stderr == ""
}
