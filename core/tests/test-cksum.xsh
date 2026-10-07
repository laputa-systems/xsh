type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Capture the real applet and its stdin without involving an external checksum.
proc invoke(ctx: TestContext, args: List[Str], input: Bytes) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "cksum-run")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/cksum.xsh"
  let plan = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--"].extend(args),
    root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_cksum_stdin_known_vector { |ctx|
  let result = invoke(ctx, [], b"abc")?
  assert result.status == 0
  assert result.stdout == b"1219131554 3\n"
  assert result.stderr == ""
}

test test_cksum_continues_after_unreadable_file { |ctx|
  let file = test.temp_file(ctx, name: "cksum-data", contents: b"abc")?
  let missing = test.temp_path(ctx, name: "cksum-missing")
  let result = invoke(ctx, [missing.display(), file.display()], b"")?
  assert result.status == 1
  assert result.stdout.len() > 0
  assert result.stderr.find("No such file or directory") != null
}

test test_cksum_digest_modes { |ctx|
  let result = invoke(ctx, ["-a", "sha2", "-l", "224"], b"abc")?
  assert result.status == 0
  assert result.stdout == b"SHA224 (-) = 23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7\n"
  let raw = invoke(ctx, ["-a", "md5", "--raw"], b"abc")?
  assert raw.status == 0
  assert raw.stdout.len() == 16
}

test test_cksum_short_blake2b_length { |ctx|
  let short = invoke(ctx, ["--algorithm=blake2b", "--length=8"], b"abc")?
  assert short.status == 0
  assert short.stdout == b"BLAKE2b-8 (-) = 6b\n"
  assert short.stderr == ""
}

test test_cksum_length_policy_diagnostics { |ctx|
  let wrong_algorithm = invoke(ctx, ["--length=16", "--algorithm=md5"], b"")?
  assert wrong_algorithm.status == 1
  assert wrong_algorithm.stderr.find("--length is only supported with --algorithm blake2b, sha2, or sha3") != null

  let too_long = invoke(ctx, ["--length=513", "--algorithm=blake2b"], b"")?
  assert too_long.status == 1
  assert too_long.stderr.find("invalid length: '513'") != null
  assert too_long.stderr.find("maximum digest length for 'BLAKE2b' is 512 bits") != null
}

test test_cksum_check_and_text_option_diagnostics { |ctx|
  let tagged_check = invoke(ctx, ["--check", "--tag", "missing"], b"")?
  assert tagged_check.status == 1
  assert tagged_check.stdout == b""
  assert tagged_check.stderr.find("the --tag option is meaningless when verifying checksums") != null

  let binary_check = invoke(ctx, ["--check", "--binary", "missing"], b"")?
  assert binary_check.status == 1
  assert binary_check.stdout == b""
  assert binary_check.stderr.find("the --binary and --text options are meaningless when verifying checksums") != null

  let tagged_text = invoke(ctx, ["--text", "--tag", "--algorithm=md5"], b"abc")?
  assert tagged_text.status == 1
  assert tagged_text.stderr.find("--tag does not support --text mode") != null

  let text = invoke(ctx, ["--text", "--untagged", "--algorithm=md5"], b"abc")?
  assert text.status == 0
  assert text.stdout == b"900150983cd24fb0d6963f7d28e17f72  -\n"
}

test test_cksum_base64_tagged_and_untagged_verification { |ctx|
  let file = test.temp_file(ctx, name: "base64-check", contents: b"")?
  let tagged = invoke(ctx, ["--algorithm=md5", "--base64", file.display()], b"")?
  assert tagged.status == 0
  let tagged_list = test.temp_file(ctx, name: "base64-tagged-list", contents: tagged.stdout)?
  let tagged_check = invoke(ctx, ["--check", tagged_list.display()], b"")?
  assert tagged_check.status == 0
  assert tagged_check.stdout == bytes.from_text(f"{file}: OK\n")

  let untagged = invoke(ctx, ["--algorithm=md5", "--base64", "--untagged", file.display()], b"")?
  assert untagged.status == 0
  let untagged_list = test.temp_file(ctx, name: "base64-untagged-list", contents: untagged.stdout)?
  let untagged_check = invoke(ctx, ["--algorithm=md5", "--check", untagged_list.display()], b"")?
  assert untagged_check.status == 0
  assert untagged_check.stdout == bytes.from_text(f"{file}: OK\n")
}

test test_cksum_sha2_check_accepts_tag_aliases_without_length { |ctx|
  let file = test.temp_file(ctx, name: "sha2-check", contents: b"")?
  let checks = bytes.from_text(f"SHA256 ({file}) = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\nSHA2-256 ({file}) = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n")
  let checked = invoke(ctx, ["--check", "--algorithm=sha2"], checks)?
  assert checked.status == 0
  assert checked.stdout == bytes.from_text(f"{file}: OK\n{file}: OK\n")
  assert checked.stderr == ""

  let sha1_lines = bytes.from_text(f"SHA1 ({file}) = da39a3ee5e6b4b0d3255bfef95601890afd80709\nda39a3ee5e6b4b0d3255bfef95601890afd80709  {file}\n")
  let rejected_sha1 = invoke(ctx, ["--check", "--algorithm=sha2"], sha1_lines)?
  assert rejected_sha1.status == 1
  assert rejected_sha1.stderr.find("no properly formatted checksum lines found") != null

  let untagged_sha3 = bytes.from_text(f"a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a  {file}\n")
  let untagged_sha3_check = invoke(ctx, ["--check", "--algorithm=sha3"], untagged_sha3)?
  assert untagged_sha3_check.status == 0
  assert untagged_sha3_check.stdout == bytes.from_text(f"{file}: OK\n")

  let blake2b_file = test.temp_file(ctx, name: "blake2b-default", contents: b"abc")?
  let blake2b = bytes.from_text(f"BLAKE2b ({blake2b_file}) = ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923\n")
  let blake2b_list = test.temp_file(ctx, name: "blake2b-default-list", contents: blake2b)?
  let blake2b_check = invoke(ctx, ["--check", blake2b_list.display()], b"")?
  assert blake2b_check.status == 0
  assert blake2b_check.stdout == bytes.from_text(f"{blake2b_file}: OK\n")
}

test test_cksum_extended_digest_algorithms { |ctx|
  let sha3 = invoke(ctx, ["--algorithm=sha3", "--length=256"], b"")?
  assert sha3.status == 0
  assert sha3.stdout == b"SHA3-256 (-) = a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a\n"

  let sm3 = invoke(ctx, ["--algorithm=sm3"], b"abc")?
  assert sm3.status == 0
  assert sm3.stdout == b"SM3 (-) = 66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0\n"

  let blake3 = invoke(ctx, ["--algorithm=blake3"], b"foo")?
  assert blake3.status == 0
  assert blake3.stdout == b"BLAKE3-256 (-) = 04e0bb39f30b1a3feb89f536c93be15055482df748674b00d26e5a75777702e9\n"

  let short_blake3 = invoke(ctx, ["--algorithm=blake3", "--length=8"], b"foo")?
  assert short_blake3.status == 0
  assert short_blake3.stdout == b"BLAKE3-8 (-) = 04\n"

  let shake128 = invoke(ctx, ["--algorithm=shake128"], b"xxx")?
  assert shake128.status == 0
  assert shake128.stdout == b"SHAKE128-256 (-) = ac8549b2861a151896ab721bd29d7a20c1a3d1f75b31266f786f20d963fb0fdf\n"

  let short_shake128 = invoke(ctx, ["--algorithm=shake128", "--length=8"], b"xxx")?
  assert short_shake128.status == 0
  assert short_shake128.stdout == b"SHAKE128-8 (-) = ac\n"

  let shake256_short = invoke(ctx, ["--algorithm=shake256", "--length=1"], b"xxx")?
  assert shake256_short.status == 0
  assert shake256_short.stdout == b"SHAKE256-1 (-) = 01\n"
}

test test_cksum_shake_default_tags_verify_without_length { |ctx|
  let file = test.temp_file(ctx, name: "shake-data", contents: b"xxx")?
  let sums = bytes.from_text(f"SHAKE128 ({file}) = ac8549b2861a151896ab721bd29d7a20c1a3d1f75b31266f786f20d963fb0fdf\nSHAKE256 ({file}) = 2fa631503c3ea5fe85131dbfa24805185474740e6dcb5f2a64f69d932bcb55f7b24958f3e3c4cc0e71f1fe6f054cd3fb28b9efb62b4f8f3fbe6d50d90f5c6eba\n")
  let list = test.temp_file(ctx, name: "shake-list", contents: sums)?
  let checked = invoke(ctx, ["--check", list.display()], b"")?
  assert checked.status == 0
  assert checked.stdout == bytes.from_text(f"{file}: OK\n{file}: OK\n")
  assert checked.stderr == ""

  let blake3_file = test.temp_file(ctx, name: "blake3-data", contents: b"foo")?
  let blake3_short = bytes.from_text(f"BLAKE3-8 ({blake3_file}) = 04\n04  {blake3_file}\n")
  let blake3_list = test.temp_file(ctx, name: "blake3-short-list", contents: blake3_short)?
  let blake3_checked = invoke(ctx, ["--check", "--algorithm=blake3", blake3_list.display()], b"")?
  assert blake3_checked.status == 0
  assert blake3_checked.stdout == bytes.from_text(f"{blake3_file}: OK\n{blake3_file}: OK\n")

  let shake_untagged = bytes.from_text(f"ac  {file}\n")
  let shake_list = test.temp_file(ctx, name: "shake-untagged-list", contents: shake_untagged)?
  let shake_checked = invoke(ctx, ["--check", "--algorithm=shake128", shake_list.display()], b"")?
  assert shake_checked.status == 0
  assert shake_checked.stdout == bytes.from_text(f"{file}: OK\n")
}
