type Ran = {status: Int, stdout: Str, stderr: Str}

proc run_sha512sum(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "sha512sum")?
  let bin = fp"{root}/bin"
  bin.mkdir()?
  let script = fp"{bin}/sha512sum"
  fs.symlink(fp"{ctx.core_dir}/cksum.xsh", script)?
  fs.symlink(fp"{ctx.core_dir}/lib", fp"{bin}/lib")?
  fp"{root}/data".write("abc")
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_sha512sum_hashes_stdin_and_binary_mark { |ctx|
  let result = run_sha512sum(ctx, ["--binary"], b"abc")?
  assert result.status == 0
  assert result.stdout == "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f *-\n", result.stdout
}
