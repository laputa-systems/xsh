type Ran = {status: Int, stdout: Str, stderr: Str}

proc chown_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "chown-run")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/chown.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_chown_current_user { |ctx|
  let target = test.temp_file(ctx, name: "owned.txt", contents: b"payload")?
  let current = user.current()?
  let name = current.name
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chown.xsh" $name $target ?
  assert output == ""
  assert target.metadata()?.uid == current.uid
  let root = test.temp_dir(ctx, name: "owned-tree")?
  let child = fp"{root}/child.txt"
  child.write("payload")
  let recursive = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chown.xsh" -R $name $root ?
  assert recursive == ""
  assert child.metadata()?.uid == current.uid
}

test test_chown_colon_reference_and_verbose { |ctx|
  let root = test.temp_dir(ctx, name: "chown-modes")?
  let target = fp"{root}/target"
  let reference = fp"{root}/reference"
  target.write("target")
  reference.write("reference")
  let current = user.current()?
  let current_group = group.current()?
  fs.chown(target, current)
  fs.chgrp(target, current_group)

  let owner_only = chown_run(ctx, [f"{current.uid}:", target.display()])?
  assert owner_only.status == 0, owner_only.stderr
  assert fs.stat(target)?.gid == current_group.gid

  let retained = chown_run(ctx, ["-v", ":", target.display()])?
  assert retained.status == 0, retained.stderr
  assert "retained as" in retained.stdout, retained.stdout

  let copied = chown_run(ctx, ["--reference", reference.display(), target.display()])?
  assert copied.status == 0, copied.stderr
  assert fs.stat(target)?.uid == fs.stat(reference)?.uid
  assert fs.stat(target)?.gid == fs.stat(reference)?.gid
}

test test_chown_rejects_invalid_user_and_option { |ctx|
  let target = test.temp_file(ctx, name: "chown-invalid", contents: b"x")?
  let invalid_user = chown_run(ctx, ["__xsh_no_such_user__", target.display()])?
  assert invalid_user.status == 1
  assert invalid_user.stderr == "chown: invalid user: '__xsh_no_such_user__'\n", invalid_user.stderr

  let invalid_option = chown_run(ctx, ["--definitely-invalid"])?
  assert invalid_option.status == 1
  assert "unrecognized option '--definitely-invalid'" in invalid_option.stderr, invalid_option.stderr
}
