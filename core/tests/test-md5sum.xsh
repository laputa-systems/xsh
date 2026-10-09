type Ran = {status: Int, stdout: Str, stderr: Str}

proc run_md5sum(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "md5sum")?
  let bin = fp"{root}/bin"
  bin.mkdir()?
  let script = fp"{bin}/md5sum"
  fs.symlink(fp"{ctx.core_dir}/cksum.xsh", script)?
  fs.symlink(fp"{ctx.core_dir}/lib", fp"{bin}/lib")?
  fp"{root}/data".write("abc")
  fp"{root}/sums".write("900150983cd24fb0d6963f7d28e17f72  data\n")
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_md5sum_hashes_files_and_stdin { |ctx|
  let file = run_md5sum(ctx, ["data"])?
  assert file.status == 0 and file.stdout == "900150983cd24fb0d6963f7d28e17f72  data\n", file.stdout
  let result = run_md5sum(ctx, ["-"], b"abc")?
  assert result.status == 0 and result.stdout == "900150983cd24fb0d6963f7d28e17f72  -\n", result.stdout
  assert result.stderr == ""
}

test test_md5sum_check_and_common_flags { |ctx|
  let result = run_md5sum(ctx, ["--check", "--quiet", "sums"])?
  assert result.status == 0 and result.stdout == "" and result.stderr == "", result.stderr
  let tagged = run_md5sum(ctx, ["--tag", "data"])?
  assert tagged.stdout == "MD5 (data) = 900150983cd24fb0d6963f7d28e17f72\n", tagged.stdout
  let version = run_md5sum(ctx, ["--version"])?
  assert version.stdout == "md5sum (XSH core) 0.0.1\n", version.stdout
}
