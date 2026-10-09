type Ran = {status: Int, stdout: Str, stderr: Str}

proc run_b2sum(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "b2sum")?
  let bin = fp"{root}/bin"
  bin.mkdir()?
  let script = fp"{bin}/b2sum"
  fs.symlink(fp"{ctx.core_dir}/cksum.xsh", script)?
  fs.symlink(fp"{ctx.core_dir}/lib", fp"{bin}/lib")?
  fp"{root}/data".write("abc")
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_b2sum_default_and_short_length { |ctx|
  let result = run_b2sum(ctx, [], b"abc")?
  assert result.status == 0
  assert result.stdout == "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923  -\n", result.stdout
  let short = run_b2sum(ctx, ["--length=8"], b"abc")?
  assert short.stdout == "6b  -\n", short.stdout
  let tagged = run_b2sum(ctx, ["--tag", "--length=128", "data"])?
  assert tagged.stdout == "BLAKE2b-128 (data) = cf4ab791c62b8d2b2109c90275287816\n", tagged.stdout
}
