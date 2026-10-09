type Ran = {status: Int, stdout: Str, stderr: Str}

proc run_sha256sum(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "sha256sum")?
  let bin = fp"{root}/bin"
  bin.mkdir()?
  let script = fp"{bin}/sha256sum"
  fs.symlink(fp"{ctx.core_dir}/cksum.xsh", script)?
  fs.symlink(fp"{ctx.core_dir}/lib", fp"{bin}/lib")?
  fp"{root}/data".write("abc")
  fp"{root}/sums".write("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  data\n")
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_sha256sum_hashes_and_verifies { |ctx|
  let result = run_sha256sum(ctx, ["-"], b"abc")?
  assert result.status == 0
  assert result.stdout == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  -\n", result.stdout
  let checked = run_sha256sum(ctx, ["-c", "sums"])?
  assert checked.status == 0 and checked.stdout == "data: OK\n", checked.stdout
}
