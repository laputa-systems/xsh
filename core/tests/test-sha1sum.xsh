type Ran = {status: Int, stdout: Str, stderr: Str}

proc run_sha1sum(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "sha1sum")?
  let bin = fp"{root}/bin"
  bin.mkdir()?
  let script = fp"{bin}/sha1sum"
  fs.symlink(fp"{ctx.core_dir}/cksum.xsh", script)?
  fs.symlink(fp"{ctx.core_dir}/lib", fp"{bin}/lib")?
  fp"{root}/data".write("abc")
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_sha1sum_hashes_stdin_and_emits_help { |ctx|
  let result = run_sha1sum(ctx, [], b"abc")?
  assert result.status == 0
  assert result.stdout == "a9993e364706816aba3e25717850c26c9cd0d89d  -\n", result.stdout
  assert result.stderr == ""
  let file = run_sha1sum(ctx, ["data"])?
  assert file.stdout == "a9993e364706816aba3e25717850c26c9cd0d89d  data\n", file.stdout
  let help = run_sha1sum(ctx, ["--help"])?
  assert help.status == 0 and "Usage: sha1sum" in help.stdout
}
