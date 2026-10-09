type Ran = {status: Int, stdout: Str, stdout_bytes: Bytes, stderr: Str}

proc run_cksum_in(ctx: TestContext, root: Path, args: List[Str], input = b"", setup = true) [fs, process, error] -> Result[Ran] {
  let bin = fp"{root}/bin"
  let script = fp"{bin}/cksum"
  if setup {
    bin.mkdir()?
    fs.symlink(fp"{ctx.core_dir}/cksum.xsh", script)?
    fs.symlink(fp"{ctx.core_dir}/lib", fp"{bin}/lib")?
  }
  fp"{root}/data".write("abc")
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  let stdout_bytes = out.read_bytes()?
  Ok({status: status.exit_code()?, stdout: stdout_bytes.utf8() ?? "", stdout_bytes: stdout_bytes, stderr: err.read_text()?})
}

proc run_cksum(ctx: TestContext, name: Str, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  run_cksum_in(ctx, test.temp_dir(ctx, name: f"cksum-{name}")?, args, input)
}

test test_cksum_posix_and_algorithm_outputs { |ctx|
  let posix = run_cksum(ctx, "posix", [], b"abc")?
  assert posix.status == 0 and posix.stdout == "1219131554 3\n", f"{posix.status}: {posix.stderr} {posix.stdout}"
  let md5 = run_cksum(ctx, "md5", ["--algorithm=md5"], b"abc")?
  assert md5.stdout == "MD5 (-) = 900150983cd24fb0d6963f7d28e17f72\n", md5.stdout
  let sha = run_cksum(ctx, "sha256", ["--algorithm=sha256", "--untagged"], b"abc")?
  assert sha.stdout == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  -\n", sha.stdout
  let sha2 = run_cksum(ctx, "sha2", ["--algorithm=sha2", "--length=224"], b"abc")?
  assert sha2.stdout == "SHA224 (-) = 23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7\n", sha2.stdout
  let sha3 = run_cksum(ctx, "sha3", ["--algorithm=sha3", "--length=256"], b"abc")?
  assert sha3.stdout == "SHA3-256 (-) = 3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532\n", sha3.stdout
  let sm3 = run_cksum(ctx, "sm3", ["--algorithm=sm3"], b"abc")?
  assert sm3.stdout == "SM3 (-) = 66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0\n", sm3.stdout
  let blake3 = run_cksum(ctx, "blake3", ["--algorithm=blake3", "--length=40"], b"abc")?
  assert blake3.stdout == "BLAKE3-40 (-) = 6437b3ac38\n", blake3.stdout
  let shake = run_cksum(ctx, "shake", ["--algorithm=shake128", "--length=3"], b"xxx")?
  assert shake.stdout == "SHAKE128-3 (-) = 04\n", shake.stdout
  let base64 = run_cksum(ctx, "md5-base64", ["--algorithm=md5", "--base64"], b"abc")?
  assert base64.stdout == "MD5 (-) = kAFQmDzST7DWlj99KOF/cg==\n", base64.stdout
  let raw = run_cksum(ctx, "md5-raw", ["--algorithm=md5", "--raw"], b"abc")?
  assert raw.stdout_bytes == b"\x90\x01\x50\x98\x3c\xd2\x4f\xb0\xd6\x96\x3f\x7d\x28\xe1\x7f\x72", raw.stdout_bytes.utf8() ?? "raw MD5 bytes differ"

  let root = test.temp_dir(ctx, name: "cksum-base64-check")?
  let sha2_output = run_cksum_in(ctx, root, ["--algorithm=sha2", "--length=224", "--base64", "--untagged", "data"], b"")?
  assert sha2_output.stdout == "Iwl9IjQF2CKGQqR3vaJVsyqtvOS9oLP342ydpw==  data\n", sha2_output.stdout
  fp"{root}/check".write(sha2_output.stdout)
  let sha2_check = run_cksum_in(ctx, root, ["--algorithm=sha2", "--check", "check"], b"", false)?
  assert sha2_check.status == 0 and sha2_check.stdout == "data: OK\n", sha2_check.stderr

  let b2_output = run_cksum_in(ctx, root, ["--algorithm=blake2b", "--length=216", "--base64", "--untagged", "data"], b"", false)?
  fp"{root}/check".write(b2_output.stdout)
  let b2_check = run_cksum_in(ctx, root, ["--algorithm=blake2b", "--check", "check"], b"", false)?
  assert b2_check.status == 0 and b2_check.stdout == "data: OK\n", b2_check.stderr

  let shake_output = run_cksum_in(ctx, root, ["--algorithm=shake128", "--length=128", "data"], b"", false)?
  fp"{root}/check".write(shake_output.stdout)
  let shake_check = run_cksum_in(ctx, root, ["--algorithm=shake128", "--check", "check"], b"", false)?
  assert shake_check.status == 0 and shake_check.stdout == "data: OK\n", shake_check.stderr

  fp"{root}/folder".mkdir()?
  let directory = run_cksum_in(ctx, root, ["--algorithm=blake3", "folder"], b"", false)?
  assert directory.status == 1 and directory.stderr == "cksum: folder: Is a directory\n", directory.stderr

  fp"{root}/status-missing".write("SM3 (missing) = 66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0\n")
  let missing_status = run_cksum_in(ctx, root, ["--check", "--status", "status-missing"], b"", false)?
  assert missing_status.status == 1 and missing_status.stdout == "", missing_status.stdout
  assert missing_status.stderr == "cksum: missing: No such file or directory\n", missing_status.stderr

  fp"{root}/status-warn".write("SM3 (data) = 0000000000000000000000000000000000000000000000000000000000000000\nbad line\n")
  let warn_after_status = run_cksum_in(ctx, root, ["--status", "--warn", "--check", "status-warn"], b"", false)?
  assert warn_after_status.status == 1 and warn_after_status.stdout == "", warn_after_status.stdout
  assert "improperly formatted SM3 checksum line" in warn_after_status.stderr, warn_after_status.stderr
  assert "WARNING: 1 line is improperly formatted" in warn_after_status.stderr, warn_after_status.stderr
  assert "WARNING: 1 computed checksum did NOT match" in warn_after_status.stderr, warn_after_status.stderr

  let status_after_warn = run_cksum_in(ctx, root, ["--warn", "--status", "--check", "status-warn"], b"", false)?
  assert status_after_warn.status == 1 and status_after_warn.stdout == "" and status_after_warn.stderr == "", status_after_warn.stderr
}
