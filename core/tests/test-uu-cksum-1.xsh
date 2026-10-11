##! Transcribed from the uutils checksum integration tests.

use support.uu as uu

# CRC reports each candidate it tries and stops at the first supported implementation.
proc crc_debug(disabled: Str = "") -> Str {
  let features = unix.cpu_features()
  var expected = ""
  for feature in ["avx512", "avx2", "pclmul"] {
    if feature in features and feature != disabled {
      expected = f"{expected}cksum: using {feature} hardware support\n"
      return expected
    }
    expected = f"{expected}cksum: {feature} support not detected\n"
  }
  expected
}

# The mixed list hashes empty stdin for four algorithms; later variants add malformed and mismatching lines.
proc check_scene(ctx: TestContext, variant: Int = 0) [fs, process, env, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  uu.write(s, "input", "9\n7\n1\n4\n2\n6\n3\n5\n8\n10\n")?
  uu.touch(s, "CHECKSUMS")?
  for args in [["-a", "sha384"], ["-a", "blake2b"], ["-a", "blake2b", "-l", "384"], ["-a", "sm3"]] {
    let r1 = uu.invoke(s, "cksum", args)?
    uu.succeeds(r1)
    uu.write_bytes(s, "CHECKSUMS", bytes.concat([uu.read(s, "CHECKSUMS")?, r1.stdout]))?
  }
  if variant >= 1 { uu.append(s, "CHECKSUMS", "# Very important comment\n")? }
  if variant >= 2 { uu.append(s, "CHECKSUMS", "invalid_line\n")? }
  if variant >= 3 { uu.append(s, "CHECKSUMS", "invalid_line\n")? }
  if variant >= 4 { uu.append(s, "CHECKSUMS", "SM3 (input) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaafdb57c725157cb40b5aee8d937b8351477e\n")? }
  if variant >= 5 { uu.append(s, "CHECKSUMS", "BLAKE2b (missing-file) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaafdb57c725157cb40b5aee8d937b8351477e\n")? }
  if variant >= 6 { uu.write(s, "CHECKSUMS-missing", "SM3 (nonexistent) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaafdb57c725157cb40b5aee8d937b8351477e\n")? }
  Ok(s)
}

# origin: uutils test_cksum::check_encoding::test_check_non_utf8_comment
test test_uu_cksum_check_encoding_check_non_utf8_comment { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "empty")?
  uu.write_bytes(s, "check", b"MD5 (empty) = 1B2M2Y8AsgTpgAmY7PhCfg==\n# Comment with a non utf8 char: >>\xff<<\nSHA256 (empty) = 47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=\nBLAKE2b (empty) = eGoC90IBWQPGxv2FJVLScpEvR0DhWEdhiobiF/cfVBnSXhAxr+5YUxOJZESTTrBLkDpoWxRIt1XVb3Aa/pvizg==\n")?
  let r1 = uu.invoke(s, "cksum", ["--check", uu.at(s, "check").display()])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "empty: OK\nempty: OK\nempty: OK\n")
}

# origin: uutils test_cksum::check_encoding::test_check_non_utf8_filename
test test_uu_cksum_check_encoding_check_non_utf8_filename { |ctx|
  let s = uu.scene(ctx)?
  let name = uu.at_bytes(s, b"funky\xffname")?
  name.write("")?
  uu.write_bytes(s, "check", b"SHA256 (funky\xffname) = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n")?
  let r1 = uu.invoke(s, "cksum", ["--check", uu.at(s, "check").display()])?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"'funky'$'\\377''name': OK\n")
  uu.no_stderr(r1)
  uu.write_bytes(s, "check", b"SHA256 (funky\xffname) = ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\n")?
  let r2 = uu.invoke(s, "cksum", ["--check", uu.at(s, "check").display()])?
  uu.fails(r2)
  uu.stdout_is_bytes(r2, b"'funky'$'\\377''name': FAILED\n")
  uu.stderr_contains(r2, "1 computed checksum did NOT match")
  uu.write_bytes(s, "check", b"SHA256 (flakey\xffname) = ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\n")?
  let r3 = uu.invoke(s, "cksum", ["--check", uu.at(s, "check").display()])?
  uu.fails(r3)
  uu.stdout_is_bytes(r3, b"'flakey'$'\\377''name': FAILED open or read\n")
  uu.stderr_contains(r3, "1 listed file could not be read")
}

# origin: uutils test_cksum::check_encoding::test_quoting_in_stderr
test test_uu_cksum_check_encoding_quoting_in_stderr { |ctx|
  let s = uu.scene(ctx)?
  uu.at_bytes(s, b"FFF\xffDIR")?.mkdir()?
  uu.write_bytes(s, "check", b"SHA256 (FFF\xffFFF) = 29953405eaa3dcc41c37d1621d55b6a47eee93e05613e439e73295029740b10c\nSHA256 (FFF\xffDIR) = 29953405eaa3dcc41c37d1621d55b6a47eee93e05613e439e73295029740b10c\n")?
  let r1 = uu.invoke(s, "cksum", ["-c", "check"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_contains(r1, "'FFF'$'\\377''FFF': FAILED open or read")
  uu.stdout_contains(r1, "'FFF'$'\\377''DIR': FAILED open or read")
  uu.stderr_contains(r1, "'FFF'$'\\377''FFF': No such file or directory")
  uu.stderr_contains(r1, "'FFF'$'\\377''DIR': Is a directory")
}

# origin: uutils test_cksum::cksum_base64_encoding::test_cksum_base64_generating
test test_uu_cksum_cksum_base64_encoding_cksum_base64_generating { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "-a", "sysv", "f"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0 0 f\n")
  let r2 = uu.invoke(s, "cksum", ["--base64", "-a", "bsd", "f"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "00000     0 f\n")
  let r3 = uu.invoke(s, "cksum", ["--base64", "-a", "crc", "f"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "4294967295 0 f\n")
  let r4 = uu.invoke(s, "cksum", ["--base64", "-a", "crc32b", "f"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "0 0 f\n")
  let r5 = uu.invoke(s, "cksum", ["--base64", "-a", "md5", "f"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "MD5 (f) = 1B2M2Y8AsgTpgAmY7PhCfg==\n")
  let r6 = uu.invoke(s, "cksum", ["--base64", "-a", "sha1", "f"])?
  uu.succeeds(r6)
  uu.stdout_only(r6, "SHA1 (f) = 2jmj7l5rSw0yVb/vlWAYkK/YBwk=\n")
  let r7 = uu.invoke(s, "cksum", ["--base64", "-a", "sha224", "f"])?
  uu.succeeds(r7)
  uu.stdout_only(r7, "SHA224 (f) = 0UoCjCo6K8lHYQK7KII0xBWisB+CjqYqxbPkLw==\n")
  let r8 = uu.invoke(s, "cksum", ["--base64", "-a", "sha256", "f"])?
  uu.succeeds(r8)
  uu.stdout_only(r8, "SHA256 (f) = 47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=\n")
  let r9 = uu.invoke(s, "cksum", ["--base64", "-a", "sha384", "f"])?
  uu.succeeds(r9)
  uu.stdout_only(r9, "SHA384 (f) = OLBgp1GsljhM2TJ+sbHjaiH9txEUvgdDTAzHv2P24donTt6/529l+9Ua0vFImLlb\n")
  let r10 = uu.invoke(s, "cksum", ["--base64", "-a", "sha512", "f"])?
  uu.succeeds(r10)
  uu.stdout_only(r10, "SHA512 (f) = z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c5H0NE8XYXysP+DGNKHfuwvY7kxvUdBeoGlODJ6+SfaPg==\n")
  let r11 = uu.invoke(s, "cksum", ["--base64", "-a", "blake2b", "f"])?
  uu.succeeds(r11)
  uu.stdout_only(r11, "BLAKE2b (f) = eGoC90IBWQPGxv2FJVLScpEvR0DhWEdhiobiF/cfVBnSXhAxr+5YUxOJZESTTrBLkDpoWxRIt1XVb3Aa/pvizg==\n")
  let r12 = uu.invoke(s, "cksum", ["--base64", "-a", "sm3", "f"])?
  uu.succeeds(r12)
  uu.stdout_only(r12, "SM3 (f) = GrIdg1XPoX+OYRlIMegajyK+yMco/vt0ftA161CCqis=\n")
}

# origin: uutils test_cksum::cksum_base64_encoding::test_cksum_base64_verify
test test_uu_cksum_cksum_base64_encoding_cksum_base64_verify { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r1 = uu.invoke(s, "cksum", ["--check", "-a", "sysv"])?
  uu.fails(r1)
  uu.stderr_only(r1, "cksum: --check is not supported with --algorithm={bsd,sysv,crc,crc32b}\n")
  let r2 = uu.invoke(s, "cksum", ["--check", "-a", "bsd"])?
  uu.fails(r2)
  uu.stderr_only(r2, "cksum: --check is not supported with --algorithm={bsd,sysv,crc,crc32b}\n")
  let r3 = uu.invoke(s, "cksum", ["--check", "-a", "crc"])?
  uu.fails(r3)
  uu.stderr_only(r3, "cksum: --check is not supported with --algorithm={bsd,sysv,crc,crc32b}\n")
  let r4 = uu.invoke(s, "cksum", ["--check", "-a", "crc32b"])?
  uu.fails(r4)
  uu.stderr_only(r4, "cksum: --check is not supported with --algorithm={bsd,sysv,crc,crc32b}\n")
  let r5 = uu.invoke(s, "cksum", ["--check", "--strict"] , stdin: bytes.from_text("MD5 (f) = 1B2M2Y8AsgTpgAmY7PhCfg=="))?
  uu.succeeds(r5)
  uu.stdout_only(r5, "f: OK\n")
  let r6 = uu.invoke(s, "cksum", ["--check", "--strict"] , stdin: bytes.from_text("SHA1 (f) = 2jmj7l5rSw0yVb/vlWAYkK/YBwk="))?
  uu.succeeds(r6)
  uu.stdout_only(r6, "f: OK\n")
  let r7 = uu.invoke(s, "cksum", ["--check", "--strict"] , stdin: bytes.from_text("SHA224 (f) = 0UoCjCo6K8lHYQK7KII0xBWisB+CjqYqxbPkLw=="))?
  uu.succeeds(r7)
  uu.stdout_only(r7, "f: OK\n")
  let r8 = uu.invoke(s, "cksum", ["--check", "--strict"] , stdin: bytes.from_text("SHA256 (f) = 47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="))?
  uu.succeeds(r8)
  uu.stdout_only(r8, "f: OK\n")
  let r9 = uu.invoke(s, "cksum", ["--check", "--strict"] , stdin: bytes.from_text("SHA384 (f) = OLBgp1GsljhM2TJ+sbHjaiH9txEUvgdDTAzHv2P24donTt6/529l+9Ua0vFImLlb"))?
  uu.succeeds(r9)
  uu.stdout_only(r9, "f: OK\n")
  let r10 = uu.invoke(s, "cksum", ["--check", "--strict"] , stdin: bytes.from_text("SHA512 (f) = z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c5H0NE8XYXysP+DGNKHfuwvY7kxvUdBeoGlODJ6+SfaPg=="))?
  uu.succeeds(r10)
  uu.stdout_only(r10, "f: OK\n")
  let r11 = uu.invoke(s, "cksum", ["--check", "--strict"] , stdin: bytes.from_text("BLAKE2b (f) = eGoC90IBWQPGxv2FJVLScpEvR0DhWEdhiobiF/cfVBnSXhAxr+5YUxOJZESTTrBLkDpoWxRIt1XVb3Aa/pvizg=="))?
  uu.succeeds(r11)
  uu.stdout_only(r11, "f: OK\n")
  let r12 = uu.invoke(s, "cksum", ["--check", "--strict"] , stdin: bytes.from_text("SM3 (f) = GrIdg1XPoX+OYRlIMegajyK+yMco/vt0ftA161CCqis="))?
  uu.succeeds(r12)
  uu.stdout_only(r12, "f: OK\n")
}

# origin: uutils test_cksum::cksum_base64_encoding::test_cksum_base64_verify_truncated_eq1
test test_uu_cksum_cksum_base64_encoding_cksum_base64_verify_truncated_eq1 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r1 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("MD5 (f) = 1B2M2Y8AsgTpgAmY7PhCfg="))?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "no properly formatted checksum lines found")
  let r2 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("SHA1 (f) = 2jmj7l5rSw0yVb/vlWAYkK/YBwk"))?
  uu.fails(r2)
  uu.no_stdout(r2)
  uu.stderr_contains(r2, "no properly formatted checksum lines found")
  let r3 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("SHA224 (f) = 0UoCjCo6K8lHYQK7KII0xBWisB+CjqYqxbPkLw="))?
  uu.fails(r3)
  uu.no_stdout(r3)
  uu.stderr_contains(r3, "no properly formatted checksum lines found")
  let r4 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("SHA256 (f) = 47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU"))?
  uu.fails(r4)
  uu.no_stdout(r4)
  uu.stderr_contains(r4, "no properly formatted checksum lines found")
  let r5 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("SHA512 (f) = z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c5H0NE8XYXysP+DGNKHfuwvY7kxvUdBeoGlODJ6+SfaPg="))?
  uu.fails(r5)
  uu.no_stdout(r5)
  uu.stderr_contains(r5, "no properly formatted checksum lines found")
  let r6 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("BLAKE2b (f) = eGoC90IBWQPGxv2FJVLScpEvR0DhWEdhiobiF/cfVBnSXhAxr+5YUxOJZESTTrBLkDpoWxRIt1XVb3Aa/pvizg="))?
  uu.fails(r6)
  uu.no_stdout(r6)
  uu.stderr_contains(r6, "no properly formatted checksum lines found")
  let r7 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("SM3 (f) = GrIdg1XPoX+OYRlIMegajyK+yMco/vt0ftA161CCqis"))?
  uu.fails(r7)
  uu.no_stdout(r7)
  uu.stderr_contains(r7, "no properly formatted checksum lines found")
}

# origin: uutils test_cksum::cksum_base64_encoding::test_cksum_base64_verify_truncated_eq2
test test_uu_cksum_cksum_base64_encoding_cksum_base64_verify_truncated_eq2 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r1 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("MD5 (f) = 1B2M2Y8AsgTpgAmY7PhCfg"))?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "no properly formatted checksum lines found")
  let r2 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("SHA224 (f) = 0UoCjCo6K8lHYQK7KII0xBWisB+CjqYqxbPkLw"))?
  uu.fails(r2)
  uu.no_stdout(r2)
  uu.stderr_contains(r2, "no properly formatted checksum lines found")
  let r3 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("SHA512 (f) = z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c5H0NE8XYXysP+DGNKHfuwvY7kxvUdBeoGlODJ6+SfaPg"))?
  uu.fails(r3)
  uu.no_stdout(r3)
  uu.stderr_contains(r3, "no properly formatted checksum lines found")
  let r4 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("BLAKE2b (f) = eGoC90IBWQPGxv2FJVLScpEvR0DhWEdhiobiF/cfVBnSXhAxr+5YUxOJZESTTrBLkDpoWxRIt1XVb3Aa/pvizg"))?
  uu.fails(r4)
  uu.no_stdout(r4)
  uu.stderr_contains(r4, "no properly formatted checksum lines found")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::blake2b_216::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_blake2b_216_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "216", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "blake2b", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::blake2b_224::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_blake2b_224_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "224", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "blake2b", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::blake2b_232::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_blake2b_232_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "232", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "blake2b", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::blake2b_248::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_blake2b_248_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "248", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "blake2b", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::blake2b_256::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_blake2b_256_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "256", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "blake2b", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::blake2b_264::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_blake2b_264_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "264", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "blake2b", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::blake2b_376::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_blake2b_376_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "376", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "blake2b", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::blake2b_384::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_blake2b_384_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "384", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "blake2b", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::blake2b_392::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_blake2b_392_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "392", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "blake2b", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::blake2b_504::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_blake2b_504_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "504", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "blake2b", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::blake2b_512::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_blake2b_512_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "512", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "blake2b", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::blake2b_8::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_blake2b_8_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "8", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "blake2b", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::sha2_224::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_sha2_224_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "sha2", "-l", "224", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "sha2", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
  uu.write(s, "check", "  inp")?
  let r3 = uu.invoke(s, "cksum", ["-a", "sha2", "check"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "--algorithm=sha2 requires specifying --length")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::sha2_256::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_sha2_256_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "sha2", "-l", "256", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "sha2", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
  uu.write(s, "check", "  inp")?
  let r3 = uu.invoke(s, "cksum", ["-a", "sha2", "check"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "--algorithm=sha2 requires specifying --length")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::sha2_384::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_sha2_384_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "sha2", "-l", "384", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "sha2", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
  uu.write(s, "check", "  inp")?
  let r3 = uu.invoke(s, "cksum", ["-a", "sha2", "check"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "--algorithm=sha2 requires specifying --length")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::sha2_512::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_sha2_512_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "sha2", "-l", "512", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "sha2", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
  uu.write(s, "check", "  inp")?
  let r3 = uu.invoke(s, "cksum", ["-a", "sha2", "check"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "--algorithm=sha2 requires specifying --length")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::sha3_224::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_sha3_224_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "sha3", "-l", "224", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "sha3", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
  uu.write(s, "check", "  inp")?
  let r3 = uu.invoke(s, "cksum", ["-a", "sha3", "check"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "--algorithm=sha3 requires specifying --length")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::sha3_256::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_sha3_256_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "sha3", "-l", "256", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "sha3", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
  uu.write(s, "check", "  inp")?
  let r3 = uu.invoke(s, "cksum", ["-a", "sha3", "check"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "--algorithm=sha3 requires specifying --length")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::sha3_384::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_sha3_384_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "sha3", "-l", "384", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "sha3", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
  uu.write(s, "check", "  inp")?
  let r3 = uu.invoke(s, "cksum", ["-a", "sha3", "check"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "--algorithm=sha3 requires specifying --length")
}

# origin: uutils test_cksum::cksum_base64_untagged_encoding::sha3_512::check_length_guess
test test_uu_cksum_cksum_base64_untagged_encoding_sha3_512_check_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "sha3", "-l", "512", "--base64", "--untagged", "inp"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "check", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-a", "sha3", "--check", "check"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "inp: OK\n")
  uu.write(s, "check", "  inp")?
  let r3 = uu.invoke(s, "cksum", ["-a", "sha3", "check"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "--algorithm=sha3 requires specifying --length")
}

# origin: uutils test_cksum::cksum_check_mode::test_awkward_filename
test test_uu_cksum_cksum_check_mode_awkward_filename { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "abc (f) = abc")?
  let r1 = uu.invoke(s, "cksum", ["-a", "sha1", "abc (f) = abc"])?
  uu.succeeds(r1)
  uu.write_bytes(s, "tag-awkward.sum", r1.stdout)?
  let r2 = uu.invoke(s, "cksum", ["-c", "tag-awkward.sum"])?
  uu.succeeds(r2)
}

# origin: uutils test_cksum::cksum_check_mode::test_check_against_older_non_hex_formats
test test_uu_cksum_cksum_check_mode_check_against_older_non_hex_formats { |ctx|
  let s = check_scene(ctx, 0)?
  let r1 = uu.invoke(s, "cksum", ["-c", "-a", "crc", "CHECKSUMS"])?
  uu.fails(r1)
  let r2 = uu.invoke(s, "cksum", ["-a", "crc", "input"])?
  uu.succeeds(r2)
  uu.write_bytes(s, "CHECKSUMS.crc", r2.stdout)?
  let r3 = uu.invoke(s, "cksum", ["-c", "CHECKSUMS.crc"])?
  uu.fails(r3)
}

# origin: uutils test_cksum::cksum_check_mode::test_check_individual_digests_in_mixed_file
test test_uu_cksum_cksum_check_mode_check_individual_digests_in_mixed_file { |ctx|
  let s = check_scene(ctx, 0)?
  let r1 = uu.invoke(s, "cksum", ["--check", "-a", "sm3", "CHECKSUMS"])?
  uu.succeeds(r1)
}

# origin: uutils test_cksum::cksum_check_mode::test_check_several_files_dont_exist
test test_uu_cksum_cksum_check_mode_check_several_files_dont_exist { |ctx|
  let s = check_scene(ctx, 0)?
  let r1 = uu.invoke(s, "cksum", ["--check", "non-existing-1", "non-existing-2"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "non-existing-1: No such file or directory")
  uu.stderr_contains(r1, "non-existing-2: No such file or directory")
}

# origin: uutils test_cksum::cksum_check_mode::test_check_several_files_empty
test test_uu_cksum_cksum_check_mode_check_several_files_empty { |ctx|
  let s = check_scene(ctx, 0)?
  uu.touch(s, "empty-1")?
  uu.touch(s, "empty-2")?
  let r1 = uu.invoke(s, "cksum", ["--check", "empty-1", "empty-2"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "empty-1: no properly formatted checksum lines found")
  uu.stderr_contains(r1, "empty-2: no properly formatted checksum lines found")
}

# origin: uutils test_cksum::cksum_check_mode::test_check_strict
test test_uu_cksum_cksum_check_mode_check_strict { |ctx|
  let s = check_scene(ctx, 2)?
  let r1 = uu.invoke(s, "cksum", ["--check", "CHECKSUMS"])?
  uu.succeeds(r1)
  uu.stderr_contains(r1, "1 line is improperly formatted")
  let r2 = uu.invoke(s, "cksum", ["--strict", "--check", "CHECKSUMS"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "1 line is improperly formatted")
}

# origin: uutils test_cksum::cksum_check_mode::test_check_strict_plural_checks
test test_uu_cksum_cksum_check_mode_check_strict_plural_checks { |ctx|
  let s = check_scene(ctx, 3)?
  let r1 = uu.invoke(s, "cksum", ["--strict", "--check", "CHECKSUMS"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "2 lines are improperly formatted")
}

# origin: uutils test_cksum::cksum_check_mode::test_check_with_incorrect_checksum
test test_uu_cksum_cksum_check_mode_check_with_incorrect_checksum { |ctx|
  let s = check_scene(ctx, 4)?
  let r1 = uu.invoke(s, "cksum", ["--check", "CHECKSUMS"])?
  uu.fails(r1)
  uu.stdout_contains(r1, "input: FAILED")
  uu.stderr_contains(r1, "1 computed checksum did NOT match")
  let r2 = uu.invoke(s, "cksum", ["--strict", "--check", "CHECKSUMS"])?
  uu.fails(r2)
  uu.stdout_contains(r2, "input: FAILED")
  uu.stderr_contains(r2, "1 computed checksum did NOT match")
}

# origin: uutils test_cksum::cksum_check_mode::test_check_with_non_existing_file
test test_uu_cksum_cksum_check_mode_check_with_non_existing_file { |ctx|
  let s = check_scene(ctx, 0)?
  uu.write(s, "CHECKSUMS2", "SM3 (input2) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaafdb57c725157cb40b5aee8d937b8351477e\n")?
  let r1 = uu.invoke(s, "cksum", ["--check", "--status", "CHECKSUMS2"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "cksum: input2: No such file or directory\n")
  let r2 = uu.invoke(s, "cksum", ["--check", "--quiet", "CHECKSUMS2"])?
  uu.fails_with_code(r2, 1)
  uu.stdout_contains(r2, "input2: FAILED open or read")
  uu.stderr_contains(r2, "input2: No such file or directory")
  let r3 = uu.invoke(s, "cksum", ["--check", "CHECKSUMS2"])?
  uu.fails(r3)
  uu.stdout_contains(r3, "input2: FAILED open or read")
  uu.stderr_contains(r3, "1 listed file could not be read")
  let r4 = uu.invoke(s, "cksum", ["--strict", "--check", "CHECKSUMS2"])?
  uu.fails(r4)
  uu.stdout_contains(r4, "input2: FAILED open or read")
  uu.stderr_contains(r4, "1 listed file could not be read")
}

# origin: uutils test_cksum::cksum_check_mode::test_ignore_missing
test test_uu_cksum_cksum_check_mode_ignore_missing { |ctx|
  let s = check_scene(ctx, 6)?
  let r1 = uu.invoke(s, "cksum", ["--ignore-missing", "--check", "CHECKSUMS-missing"])?
  uu.fails(r1)
  assert "nonexistent: No such file or directory" not in r1.stdout.utf8()?
  assert "nonexistent: FAILED open or read" not in r1.stdout.utf8()?
  uu.stderr_contains(r1, "CHECKSUMS-missing: no file was verified")
}

# origin: uutils test_cksum::cksum_check_mode::test_ignore_missing_stdin
test test_uu_cksum_cksum_check_mode_ignore_missing_stdin { |ctx|
  let s = check_scene(ctx, 6)?
  let r1 = uu.invoke(s, "cksum", ["--ignore-missing", "--check"] , stdin: uu.read(s, "CHECKSUMS-missing")?)?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "'standard input': no file was verified")
}

# origin: uutils test_cksum::cksum_check_mode::test_signed_checksums
test test_uu_cksum_cksum_check_mode_signed_checksums { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_file")?
  let r1 = uu.invoke(s, "cksum", ["-a", "sha256", "test_file"])?
  uu.succeeds(r1)
  let valid = r1.stdout.utf8()?.trim()
  uu.write(s, "signed_CHECKSUMS", f"-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA256\n\n# This is a comment that should be ignored\n{valid}\n-----BEGIN PGP SIGNATURE-----\n\nxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\nxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n=PlaceHolder\n-----END PGP SIGNATURE-----")?
  let r2 = uu.invoke(s, "cksum", ["--check", "signed_CHECKSUMS"])?
  uu.succeeds(r2)
}

# origin: uutils test_cksum::cksum_check_mode::test_status
test test_uu_cksum_cksum_check_mode_status { |ctx|
  let s = check_scene(ctx, 0)?
  let r1 = uu.invoke(s, "cksum", ["--status", "--check", "CHECKSUMS"])?
  uu.succeeds(r1)
  uu.no_output(r1)
}

# origin: uutils test_cksum::cksum_check_mode::test_status_and_ignore_missing
test test_uu_cksum_cksum_check_mode_status_and_ignore_missing { |ctx|
  let s = check_scene(ctx, 6)?
  let r1 = uu.invoke(s, "cksum", ["--status", "--ignore-missing", "--check", "CHECKSUMS"])?
  uu.fails(r1)
  uu.no_output(r1)
}

# origin: uutils test_cksum::cksum_check_mode::test_status_and_warn
test test_uu_cksum_cksum_check_mode_status_and_warn { |ctx|
  let s = check_scene(ctx, 6)?
  let r1 = uu.invoke(s, "cksum", ["--status", "--warn", "--check", "CHECKSUMS"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "CHECKSUMS: 9: improperly formatted BLAKE2b checksum line")
  uu.stderr_contains(r1, "WARNING: 3 lines are improperly formatted")
  uu.stderr_contains(r1, "WARNING: 1 computed checksum did NOT match")
  let r2 = uu.invoke(s, "cksum", ["--warn", "--status", "--check", "CHECKSUMS"])?
  uu.fails(r2)
  assert "CHECKSUMS: 9: improperly formatted BLAKE2b checksum line" not in r2.stderr.utf8()?
  assert "WARNING: 3 lines are improperly formatted" not in r2.stderr.utf8()?
  assert "WARNING: 1 computed checksum did NOT match" not in r2.stderr.utf8()?
}

# origin: uutils test_cksum::cksum_check_mode::test_status_warn_and_ignore_missing
test test_uu_cksum_cksum_check_mode_status_warn_and_ignore_missing { |ctx|
  let s = check_scene(ctx, 6)?
  let r1 = uu.invoke(s, "cksum", ["--status", "--warn", "--ignore-missing", "--check", "CHECKSUMS-missing"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "CHECKSUMS-missing: no file was verified")
  assert "nonexistent: No such file or directory" not in r1.stdout.utf8()?
}

# origin: uutils test_cksum::cksum_check_mode::test_status_with_comment
test test_uu_cksum_cksum_check_mode_status_with_comment { |ctx|
  let s = check_scene(ctx, 1)?
  let r1 = uu.invoke(s, "cksum", ["--status", "--check", "CHECKSUMS"])?
  uu.succeeds(r1)
  uu.no_output(r1)
}

# origin: uutils test_cksum::cksum_check_mode::test_status_with_directory
test test_uu_cksum_cksum_check_mode_status_with_directory { |ctx|
  let s = check_scene(ctx, 0)?
  uu.mkdir(s, "dir")?
  uu.write(s, "CHECKSUMS2", "SM3 (dir) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaafdb57c725157cb40b5aee8d937b8351477e\n")?
  let r1 = uu.invoke(s, "cksum", ["--check", "--status", "CHECKSUMS2"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "cksum: dir: Is a directory\n")
}

# origin: uutils test_cksum::cksum_check_mode::test_status_with_errors
test test_uu_cksum_cksum_check_mode_status_with_errors { |ctx|
  let s = check_scene(ctx, 4)?
  let r1 = uu.invoke(s, "cksum", ["--status", "--check", "CHECKSUMS"])?
  uu.fails(r1)
  uu.no_output(r1)
}

# origin: uutils test_cksum::cksum_check_mode::test_tagged_invalid_length
test test_uu_cksum_cksum_check_mode_tagged_invalid_length { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "sha2-bad-length.sum", "SHA2-128 (/dev/null) = 38b060a751ac96384cd9327eb1b1e36a")?
  let r1 = uu.invoke(s, "cksum", ["--check", "sha2-bad-length.sum"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "sha2-bad-length.sum: no properly formatted checksum lines found")
}

# origin: uutils test_cksum::cksum_check_mode::test_untagged_base64_matching_tag
test test_uu_cksum_cksum_check_mode_untagged_base64_matching_tag { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "tag-prefix.sum", "SHA1+++++++++++++++++++++++=  /dev/null")?
  let r1 = uu.invoke(s, "cksum", ["--check", "-a", "sha1", "tag-prefix.sum"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "WARNING: 1 computed checksum did NOT match")
}

# origin: uutils test_cksum::cksum_check_mode::test_warn
test test_uu_cksum_cksum_check_mode_warn { |ctx|
  let s = check_scene(ctx, 5)?
  let r1 = uu.invoke(s, "cksum", ["--warn", "--check", "CHECKSUMS"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "CHECKSUMS: 6: improperly formatted SM3 checksum line")
  uu.stderr_contains(r1, "CHECKSUMS: 9: improperly formatted BLAKE2b checksum line")
}

# origin: uutils test_cksum::debug_flag::test_debug_flag
test test_uu_cksum_debug_flag_debug_flag { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["--debug", "lorem_ipsum.txt"])?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/crc_single_file.expected".read_bytes()?)
  uu.stderr_is(r1, crc_debug())
  let r2 = uu.invoke(s, "cksum", ["--debug", "-a", "md5", "lorem_ipsum.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/md5_single_file.expected".read_bytes()?)
  uu.no_stderr(r2)
  let r3 = uu.invoke(s, "cksum", ["--debug"] , stdin: b"test")?
  uu.succeeds(r3)
  uu.stderr_is(r3, crc_debug())
  let r4 = uu.invoke(s, "cksum", ["--debug", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r4)
  uu.stdout_is_bytes(r4, fp"{ctx.core_dir}/tests/data/uutils/cksum/crc_multiple_files.expected".read_bytes()?)
  uu.stderr_is(r4, crc_debug())
}

# origin: uutils test_cksum::debug_flag::test_debug_with_algorithms
test test_uu_cksum_debug_flag_debug_with_algorithms { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--debug", "-a", "sha256", "lorem_ipsum.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  let r2 = uu.invoke(s, "cksum", ["--debug", "-a", "blake2b", "lorem_ipsum.txt"])?
  uu.succeeds(r2)
  uu.no_stderr(r2)
  let r3 = uu.invoke(s, "cksum", ["--debug", "-a", "blake2b", "--length", "256", "lorem_ipsum.txt"])?
  uu.succeeds(r3)
  uu.no_stderr(r3)
  let r4 = uu.invoke(s, "cksum", ["--debug", "-a", "sha1", "lorem_ipsum.txt"])?
  uu.succeeds(r4)
  uu.no_stderr(r4)
}

# origin: uutils test_cksum::debug_flag::test_debug_with_glibc_tunables
test test_uu_cksum_debug_flag_debug_with_glibc_tunables { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  if "avx2" in unix.cpu_features() {
  let r1 = uu.invoke(s, "cksum", ["--debug", "lorem_ipsum.txt"], vars: {GLIBC_TUNABLES: "glibc.cpu.hwcaps=-AVX2"})?
    uu.succeeds(r1)
    uu.stderr_is(r1, crc_debug("avx2"))
  let r2 = uu.invoke(s, "cksum", ["--debug", "lorem_ipsum.txt"], vars: {GLIBC_TUNABLES: "glibc.cpu.hwcaps=-avx2"})?
    uu.succeeds(r2)
    uu.stderr_is(r2, crc_debug(""))
  let r3 = uu.invoke(s, "cksum", ["--debug", "lorem_ipsum.txt"], vars: {GLIBC_TUNABLES: "glibc.cpu.hwcaps=-AVX2:glibc.cpu.hwcaps=-AVX512F"})?
    uu.succeeds(r3)
    uu.stderr_is(r3, crc_debug("avx512"))
  let r4 = uu.invoke(s, "cksum", ["--debug", "lorem_ipsum.txt"], vars: {GLIBC_TUNABLES: "glibc.cpu.hwcaps=-AVX2 "})?
    uu.succeeds(r4)
    uu.stderr_is(r4, crc_debug(""))
  }
}

# origin: uutils test_cksum::format_mix::test_check_algo_non_algo
test test_uu_cksum_format_mix_check_algo_non_algo { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.touch(s, "bar")?
  let r1 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("BLAKE2b (bar) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce\n786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce  foo"))?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "bar: OK")
  uu.stderr_contains(r1, "cksum: WARNING: 1 line is improperly formatted")
}

# origin: uutils test_cksum::format_mix::test_check_cli_algo_non_algo
test test_uu_cksum_format_mix_check_cli_algo_non_algo { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.touch(s, "bar")?
  let r1 = uu.invoke(s, "cksum", ["--check", "--algo=blake2b"] , stdin: bytes.from_text("BLAKE2b (bar) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce\n786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce  foo"))?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "bar: OK\nfoo: OK")
  uu.no_stderr(r1)
}

# origin: uutils test_cksum::format_mix::test_check_cli_non_algo_algo
test test_uu_cksum_format_mix_check_cli_non_algo_algo { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.touch(s, "bar")?
  let r1 = uu.invoke(s, "cksum", ["--check", "--algo=blake2b"] , stdin: bytes.from_text("786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce  foo\nBLAKE2b (bar) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce"))?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "foo: OK\nbar: OK")
  uu.no_stderr(r1)
}

# origin: uutils test_cksum::format_mix::test_check_non_algo_algo
test test_uu_cksum_format_mix_check_non_algo_algo { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.touch(s, "bar")?
  let r1 = uu.invoke(s, "cksum", ["--check"] , stdin: bytes.from_text("786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce  foo\nBLAKE2b (bar) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce"))?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "bar: OK")
  uu.stderr_contains(r1, "cksum: WARNING: 1 line is improperly formatted")
}

# origin: uutils test_cksum::output_format::test_text_binary
test test_uu_cksum_output_format_text_binary { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r1 = uu.invoke(s, "cksum", ["--text", "--binary", "-a", "md5", uu.at(s, "f").display()])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "f) = d41d8cd98f00b204e9800998ecf8427e")
}

# origin: uutils test_cksum::output_format::test_text_binary_untagged
test test_uu_cksum_output_format_text_binary_untagged { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r1 = uu.invoke(s, "cksum", ["--text", "--binary", "--untagged", "-a", "md5", uu.at(s, "f").display()])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "d41d8cd98f00b204e9800998ecf8427e *")
}

# origin: uutils test_cksum::output_format::test_text_no_untagged
test test_uu_cksum_output_format_text_no_untagged { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r1 = uu.invoke(s, "cksum", ["--text", "-a", "md5", uu.at(s, "f").display()])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "--text mode is only supported with --untagged")
}

# origin: uutils test_cksum::output_format::test_text_tag
test test_uu_cksum_output_format_text_tag { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r1 = uu.invoke(s, "cksum", ["--text", "--tag", "-a", "md5", uu.at(s, "f").display()])?
  uu.fails(r1)
}

# origin: uutils test_cksum::test_algorithm_multiple_files::case_01_sysv
test test_uu_cksum_test_algorithm_multiple_files_case_01_sysv { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=sysv", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=sysv' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=sysv", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/sysv_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_multiple_files::case_02_bsd
test test_uu_cksum_test_algorithm_multiple_files_case_02_bsd { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=bsd", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=bsd' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=bsd", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/bsd_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_multiple_files::case_03_crc
test test_uu_cksum_test_algorithm_multiple_files_case_03_crc { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=crc", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=crc' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=crc", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/crc_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_multiple_files::case_04_md5
test test_uu_cksum_test_algorithm_multiple_files_case_04_md5 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=md5", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=md5' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=md5", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/md5_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_multiple_files::case_05_sha1
test test_uu_cksum_test_algorithm_multiple_files_case_05_sha1 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=sha1", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=sha1' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=sha1", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha1_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_multiple_files::case_06_sha224
test test_uu_cksum_test_algorithm_multiple_files_case_06_sha224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=sha224", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=sha224' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=sha224", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha224_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_multiple_files::case_07_sha256
test test_uu_cksum_test_algorithm_multiple_files_case_07_sha256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=sha256", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=sha256' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=sha256", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha256_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_multiple_files::case_08_sha384
test test_uu_cksum_test_algorithm_multiple_files_case_08_sha384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=sha384", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=sha384' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=sha384", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha384_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_multiple_files::case_09_sha512
test test_uu_cksum_test_algorithm_multiple_files_case_09_sha512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=sha512", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=sha512' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=sha512", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha512_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_multiple_files::case_10_blake2b
test test_uu_cksum_test_algorithm_multiple_files_case_10_blake2b { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=blake2b", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=blake2b' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=blake2b", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/blake2b_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_multiple_files::case_12_sm3
test test_uu_cksum_test_algorithm_multiple_files_case_12_sm3 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=sm3", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=sm3' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=sm3", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/sm3_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_single_file::case_01_sysv
test test_uu_cksum_test_algorithm_single_file_case_01_sysv { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=sysv", "lorem_ipsum.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=sysv' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=sysv", "lorem_ipsum.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/sysv_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_single_file::case_02_bsd
test test_uu_cksum_test_algorithm_single_file_case_02_bsd { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=bsd", "lorem_ipsum.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=bsd' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=bsd", "lorem_ipsum.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/bsd_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_single_file::case_03_crc
test test_uu_cksum_test_algorithm_single_file_case_03_crc { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=crc", "lorem_ipsum.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=crc' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=crc", "lorem_ipsum.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/crc_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_single_file::case_04_md5
test test_uu_cksum_test_algorithm_single_file_case_04_md5 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=md5", "lorem_ipsum.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=md5' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=md5", "lorem_ipsum.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/md5_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_single_file::case_05_sha1
test test_uu_cksum_test_algorithm_single_file_case_05_sha1 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=sha1", "lorem_ipsum.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=sha1' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=sha1", "lorem_ipsum.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha1_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_single_file::case_06_sha224
test test_uu_cksum_test_algorithm_single_file_case_06_sha224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=sha224", "lorem_ipsum.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=sha224' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=sha224", "lorem_ipsum.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha224_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_single_file::case_07_sha256
test test_uu_cksum_test_algorithm_single_file_case_07_sha256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=sha256", "lorem_ipsum.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=sha256' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=sha256", "lorem_ipsum.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha256_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_algorithm_single_file::case_08_sha384
test test_uu_cksum_test_algorithm_single_file_case_08_sha384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a=sha384", "lorem_ipsum.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid argument '=sha384' for '--algorithm'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=sha384", "lorem_ipsum.txt"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha384_single_file.expected".read_bytes()?)
}
