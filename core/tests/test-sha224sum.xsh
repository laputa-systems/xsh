type Ran = {status: Int, stdout: Str, stderr: Str}

proc run_sha224sum(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "sha224sum")?
  let bin = fp"{root}/bin"
  bin.mkdir()?
  let script = fp"{bin}/sha224sum"
  fs.symlink(fp"{ctx.core_dir}/cksum.xsh", script)?
  fs.symlink(fp"{ctx.core_dir}/lib", fp"{bin}/lib")?
  fp"{root}/data".write("abc")
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_sha224sum_hashes_stdin { |ctx|
  let result = run_sha224sum(ctx, [], b"abc")?
  assert result.status == 0
  assert result.stdout == "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7  -\n", result.stdout
  let file = run_sha224sum(ctx, ["data"])?
  assert file.stdout == "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7  data\n", file.stdout
}
