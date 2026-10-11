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

proc invoke_with_tunables(ctx: TestContext, args: List[Str], input: Bytes, tunables: Str) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "cksum-run-tunables")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/cksum.xsh"
  let plan = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--"].extend(args),
    root, {LC_ALL: "C", GLIBC_TUNABLES: tunables}, input, out, err)
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

test test_cksum_algorithm_equals_and_repeated_options { |ctx|
  let result = invoke(ctx, ["-asha1", "--algo=sha256", "-amd5"], b"abc")?
  assert result.status == 0
  assert result.stdout == b"MD5 (-) = 900150983cd24fb0d6963f7d28e17f72\n"
  assert result.stderr == ""
}

test test_cksum_invalid_option_uses_gnu_exit_status { |ctx|
  let result = invoke(ctx, ["--definitely-invalid"], b"")?
  assert result.status == 1
  assert result.stderr.find("unrecognized option") != null
}

test test_cksum_debug_reports_cpu_features_and_honors_tunables { |ctx|
  let info = system.uname()?
  let features = unix.cpu_features()
  let normal = invoke(ctx, [], b"test")?
  let debug = invoke(ctx, ["--debug"], b"test")?
  assert debug.status == 0
  assert debug.stdout == normal.stdout

  let names = if info.machine.starts_with("x86") or (info.machine.starts_with("i") and info.machine.ends_with("86")) { ["avx512", "avx2", "pclmul"] } else if info.machine == "aarch64" { ["vmull"] } else { [] }
  var expected = ""
  for name in names {
    let enabled = if name == "avx512" or name == "avx2" { name in features and "vpclmulqdq" in features } else if name == "pclmul" { name in features and "avx" in features } else { name in features }
    expected += if enabled { f"cksum: using {name} hardware support\n" } else { f"cksum: {name} support not detected\n" }
    if enabled { break }
  }
  assert debug.stderr == expected

  let crypto = invoke(ctx, ["--debug", "--algorithm=md5"], b"test")?
  assert crypto.status == 0
  assert crypto.stderr == ""

  if "avx512" in features {
    let disabled = invoke_with_tunables(ctx, ["--debug"], b"test", "glibc.cpu.hwcaps=-AVX512F")?
    assert disabled.status == 0
    assert disabled.stderr.starts_with("cksum: avx512 support not detected\n")
    let reset = invoke_with_tunables(ctx, ["--debug"], b"test", "glibc.cpu.hwcaps=-AVX512F:glibc.cpu.hwcaps=")?
    assert reset.stderr == debug.stderr
  }
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

test test_cksum_invalid_sha3_lengths_have_gnu_diagnostics { |ctx|
  for length in ["216", "248", "376", "504", "513", "1024", "18446744073709551616"] {
    let expected = f"cksum: invalid length: '{length}'\ncksum: digest length for 'SHA3' must be 224, 256, 384, or 512\n"
    let generate = invoke(ctx, ["--algorithm=sha3", "--length", length], b"")?
    assert generate.status == 1
    assert generate.stderr == expected

    let check = invoke(ctx, ["--algorithm=sha3", "--length", length, "--check"], b"")?
    assert check.status == 1
    assert check.stderr == expected
  }
}

test test_cksum_invalid_sha3_tag_reports_sha3_family { |ctx|
  let file = test.temp_file(ctx, name: "sha3-malformed-tag", contents: b"")?
  let list = test.temp_file(ctx, name: "sha3-malformed-list", contents: bytes.from_text(
    f"SHA3-248 ({file}) = b4753bf1696fda712821b665494c89090ffb0e87b8645559ad9f5db25b42d4f3\n"))?
  let result = invoke(ctx, ["--check", "--warn", list.display()], b"")?
  assert result.status == 1
  assert result.stderr.find(f"{list}: 1: improperly formatted SHA3 checksum line") != null
  assert result.stderr.find("SHA3-248 checksum line") == null
}

test test_cksum_help_lists_supported_algorithms { |ctx|
  let result = invoke(ctx, ["--help"], b"")?
  assert result.status == 0
  let output = result.stdout.utf8()?
  let digest_help = "DIGEST determines the digest algorithm and default output format:\n  - sysv:     (equivalent to sum -s)\n  - bsd:      (equivalent to sum -r)\n  - crc:      (equivalent to cksum)\n  - crc32b:   (only available through cksum)\n  - md5:      (equivalent to md5sum)\n  - sha1:     (equivalent to sha1sum)\n  - sha2:     (equivalent to sha{224,256,384,512}sum)\n  - sha3:     (only available through cksum)\n  - blake2b:  (equivalent to b2sum)\n  - sm3:      (only available through cksum)"
  assert output.find(digest_help) != null
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

test test_cksum_check_algorithm_tag_aliases { |ctx|
  let file = test.temp_file(ctx, name: "algorithm-tag-data", contents: b"")?
  let sums = bytes.from_text(f"BLAKE3 ({file}) = af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262\nBLAKE2b-512 ({file}) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce\n")
  let list = test.temp_file(ctx, name: "algorithm-tag-list", contents: sums)?

  let inferred = invoke(ctx, ["--check", list.display()], b"")?
  assert inferred.status == 0
  assert inferred.stdout == bytes.from_text(f"{file}: OK\n{file}: OK\n")

  let explicit = bytes.from_text(f"BLAKE3 ({file}) = af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262\n")
  let explicit_list = test.temp_file(ctx, name: "blake3-default-tag-list", contents: explicit)?
  let explicit_check = invoke(ctx, ["--check", "--algorithm=blake3", explicit_list.display()], b"")?
  assert explicit_check.status == 0
  assert explicit_check.stdout == bytes.from_text(f"{file}: OK\n")
}

test test_cksum_check_warn_status_order_and_missing_files { |ctx|
  let empty = test.temp_file(ctx, name: "status-warn-empty", contents: b"")?
  let lines = bytes.from_text(f"MD5 ({empty}) = d41d8cd98f00b204e9800998ecf8427e\nmalformed\n")
  let list = test.temp_file(ctx, name: "status-warn-list", contents: lines)?

  let warn_after_status = invoke(ctx, ["--status", "--warn", "--check", list.display()], b"")?
  assert warn_after_status.status == 0
  assert warn_after_status.stdout == b""
  assert warn_after_status.stderr.find("improperly formatted") != null

  let status_after_warn = invoke(ctx, ["--warn", "--status", "--check", list.display()], b"")?
  assert status_after_warn.status == 0
  assert status_after_warn.stdout == b""
  assert status_after_warn.stderr == ""

  let missing = bytes.from_text("SHA256 (missing) = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n")
  let missing_list = test.temp_file(ctx, name: "status-missing-list", contents: missing)?
  let status_missing = invoke(ctx, ["--check", "--status", missing_list.display()], b"")?
  assert status_missing.status == 1
  assert status_missing.stdout == b""
  assert status_missing.stderr.find("missing: No such file or directory") != null

  let ignored = invoke(ctx, ["--check", "--ignore-missing"], missing)?
  assert ignored.status == 1
  assert ignored.stderr.find("'standard input': no file was verified") != null
}

test test_cksum_check_diagnostic_labels_and_algorithm_errors { |ctx|
  let empty = test.temp_file(ctx, name: "diagnostic-context", contents: b"")?
  let malformed = bytes.from_text(f"SM3 ({empty}) = 1ab21d8355cfa17f8e61194831e81a8f22bec8c728fefb747ed035eb5082aa2b\nmalformed\n")
  let warned = invoke(ctx, ["--check", "--warn"], malformed)?
  assert warned.status == 0
  assert warned.stderr.find("improperly formatted SM3 checksum line") != null

  let numeric = invoke(ctx, ["--check", "--algorithm=crc"], b"")?
  assert numeric.status == 1
  assert numeric.stderr == "cksum: --check is not supported with --algorithm={bsd,sysv,crc,crc32b}\n"

  let conflict = invoke(ctx, ["--base64", "--raw"], b"")?
  assert conflict.status == 1
  assert conflict.stderr == "cksum: --base64 and --raw are mutually exclusive\nTry 'cksum --help' for more information.\n"
}

test test_cksum_default_check_rejects_untagged_digest_lines { |ctx|
  let untagged_md5 = bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  empty\n")
  let rejected = invoke(ctx, ["--check"], untagged_md5)?
  assert rejected.status == 1
  assert rejected.stderr.find("no properly formatted checksum lines found") != null

  let empty = test.temp_file(ctx, name: "mixed-format-empty", contents: b"")?
  let mixed = bytes.from_text(f"BLAKE2b ({empty}) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce\n786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce  {empty}\n")
  let mixed_list = test.temp_file(ctx, name: "mixed-format-list", contents: mixed)?
  let checked = invoke(ctx, ["--check", mixed_list.display()], b"")?
  assert checked.status == 0
  assert checked.stdout == bytes.from_text(f"{empty}: OK\n")
  assert checked.stderr.find("WARNING: 1 line is improperly formatted") != null
}

test test_cksum_check_handles_non_utf8_names_and_comments { |ctx|
  let root = test.temp_dir(ctx, name: "non-utf8-check")?
  let filename = Path.parse_bytes(bytes.concat([root.bytes(), b"/funky\xffname"]))?
  filename.write("")
  let missing = Path.parse_bytes(bytes.concat([root.bytes(), b"/FFF\xffFFF"]))?
  let directory = Path.parse_bytes(bytes.concat([root.bytes(), b"/FFF\xffDIR"]))?
  directory.mkdir()

  let lines = bytes.concat([
    b"# ignored comment with invalid byte: \xff\nSHA256 (",
    filename.bytes(),
    b") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\nSHA256 (",
    missing.bytes(),
    b") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\nSHA256 (",
    directory.bytes(),
    b") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n",
  ])
  let list = test.temp_file(ctx, name: "non-utf8-check-list", contents: lines)?
  let checked = invoke(ctx, ["--check", list.display()], b"")?
  let output = checked.stdout.utf8()?

  assert checked.status == 1
  assert output.find("funky'$'\\377''name': OK\n") != null
  assert output.find("FFF'$'\\377''FFF': FAILED open or read\n") != null
  assert output.find("FFF'$'\\377''DIR': FAILED open or read\n") != null
  assert checked.stderr.find("FFF'$'\\377''FFF': No such file or directory") != null
  assert checked.stderr.find("FFF'$'\\377''DIR': Is a directory") != null
}


test test_cksum_short_attached_equals_is_part_of_value { |ctx|
  for algorithm in ["md5", "crc", "sha2", "blake2b"] {
    let result = invoke(ctx, [f"-a={algorithm}"], b"abc")?
    assert result.status == 1
    assert result.stdout == b""
    assert result.stderr.starts_with(f"cksum: invalid argument '={algorithm}' for '--algorithm'\nValid arguments are:\n")
  }
  let length = invoke(ctx, ["-a", "blake2b", "-l=8"], b"abc")?
  assert length.status == 1
  assert length.stdout == b""
  assert length.stderr.find("invalid length: '=8'") != null
}

test test_cksum_debug_requires_crc_instruction_features { |ctx|
  let machine = system.uname()?.machine
  if !machine.starts_with("x86") and !(machine.starts_with("i") and machine.ends_with("86")) { test.skip("x86 CRC instruction selection") }
  let disabled = invoke_with_tunables(ctx, ["--debug"], b"abc", "glibc.cpu.hwcaps=-VPCLMULQDQ,-AVX")?
  assert disabled.status == 0
  assert disabled.stdout == b"1219131554 3\n"
  assert disabled.stderr == "cksum: avx512 support not detected\ncksum: avx2 support not detected\ncksum: pclmul support not detected\n"
  let pclmul_disabled = invoke_with_tunables(ctx, ["--debug"], b"abc", "glibc.cpu.hwcaps=-VPCLMULQDQ,-PCLMULQDQ")?
  assert pclmul_disabled.stderr == disabled.stderr
}

test test_cksum_ignore_missing_requires_check_and_help_hint { |ctx|
  let result = invoke(ctx, ["--ignore-missing"], b"abc")?
  assert result.status == 1
  assert result.stdout == b""
  assert result.stderr == "cksum: the --ignore-missing option is meaningful only when verifying checksums\nTry 'cksum --help' for more information.\n"
}
