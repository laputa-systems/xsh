type Ran = {status: Int, stdout: Str, stderr: Str}

proc run_sum(ctx: TestContext, name: Str, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: f"sum-{name}")?
  let bin = fp"{root}/bin"
  bin.mkdir()?
  let script = fp"{bin}/sum"
  fs.symlink(fp"{ctx.core_dir}/cksum.xsh", script)?
  fs.symlink(fp"{ctx.core_dir}/lib", fp"{bin}/lib")?
  fp"{root}/data".write("abc")
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_sum_bsd_and_sysv_algorithms { |ctx|
  let bsd = run_sum(ctx, "bsd", [], b"abc")?
  assert bsd.status == 0 and bsd.stdout == "16556     1\n", bsd.stdout
  let sysv = run_sum(ctx, "sysv", ["-s"], b"abc")?
  assert sysv.status == 0 and sysv.stdout == "294 1\n", sysv.stdout
  let file_result = run_sum(ctx, "file", ["data"])?
  assert file_result.stdout == "16556     1 data\n", file_result.stdout
}
