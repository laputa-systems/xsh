type Ran = {status: Int, stdout: Str, stderr: Str}

proc chmod_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "chmod-run")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/chmod.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_chmod_recursive { |ctx|
  let root = test.temp_dir(ctx, name: "chmod")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  let child = fp"{dir}/child.txt"
  child.write("payload")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chmod.xsh" -R 700 $dir ?
  assert child.metadata()?.mode % 512 == 448
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chmod.xsh" 600 $child ?
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chmod.xsh" u+x,g+r $child ?
  assert child.metadata()?.mode % 512 == 480
}

test test_chmod_symbolic_negative_mode_and_reference { |ctx|
  let root = test.temp_dir(ctx, name: "chmod-symbolic")?
  let target = fp"{root}/target"
  let reference = fp"{root}/reference"
  target.write("payload")
  reference.write("reference")
  fs.chmod(target, 0o777)
  fs.chmod(reference, 0o640)

  let changed = chmod_run(ctx, ["u=rw,g=r,o=", target.display()])?
  assert changed.status == 0, changed.stderr
  assert fs.stat(target)?.mode.bit_and(0o777) == 0o640

  let minus = chmod_run(ctx, ["-w", target.display()])?
  assert minus.status == 0, minus.stderr
  assert fs.stat(target)?.mode.bit_and(0o777) == 0o440

  let copied = chmod_run(ctx, ["--reference", reference.display(), target.display()])?
  assert copied.status == 0, copied.stderr
  assert fs.stat(target)?.mode.bit_and(0o777) == 0o640
}

test test_chmod_quiet_and_gnu_option_errors { |ctx|
  let missing = fp"{test.temp_dir(ctx, name: "chmod-errors")?}/missing"
  let quiet = chmod_run(ctx, ["-f", "644", missing.display()])?
  assert quiet.status == 1
  assert quiet.stderr == ""

  let invalid = chmod_run(ctx, ["--definitely-invalid"])?
  assert invalid.status == 1
  assert invalid.stderr == "chmod: unrecognized option '--definitely-invalid'\nTry 'chmod --help' for more information.\n", invalid.stderr
}
