type Ran = {status: Int, stdout: Str, stderr: Str}

proc run_cksum(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "cksum")?
  let bin = fp"{root}/bin"
  bin.mkdir()?
  let script = fp"{bin}/cksum"
  fs.symlink(fp"{ctx.core_dir}/cksum.xsh", script)?
  fs.symlink(fp"{ctx.core_dir}/lib", fp"{bin}/lib")?
  fp"{root}/data".write("abc")
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_cksum_posix_and_algorithm_outputs { |ctx|
  let posix = run_cksum(ctx, [], b"abc")?
  assert posix.status == 0 and posix.stdout == "1219131554 3\n", f"{posix.status}: {posix.stderr} {posix.stdout}"
  let md5 = run_cksum(ctx, ["--algorithm=md5"], b"abc")?
  assert md5.stdout == "MD5 (-) = 900150983cd24fb0d6963f7d28e17f72\n", md5.stdout
  let sha = run_cksum(ctx, ["--algorithm=sha256", "--untagged"], b"abc")?
  assert sha.stdout == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  -\n", sha.stdout
  let sha2 = run_cksum(ctx, ["--algorithm=sha2", "--length=224"], b"abc")?
  assert sha2.stdout == "SHA224 (-) = 23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7\n", sha2.stdout
}
