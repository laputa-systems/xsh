type Ran = {status: Int, stdout: Str, stderr: Str}

proc run_sha384sum(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "sha384sum")?
  let bin = fp"{root}/bin"
  bin.mkdir()?
  let script = fp"{bin}/sha384sum"
  fs.symlink(fp"{ctx.core_dir}/cksum.xsh", script)?
  fs.symlink(fp"{ctx.core_dir}/lib", fp"{bin}/lib")?
  fp"{root}/data".write("abc")
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_sha384sum_hashes_stdin { |ctx|
  let result = run_sha384sum(ctx, [], b"abc")?
  assert result.status == 0
  assert result.stdout == "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7  -\n", result.stdout
  let file = run_sha384sum(ctx, ["data"])?
  assert file.stdout == "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7  data\n", file.stdout
}
