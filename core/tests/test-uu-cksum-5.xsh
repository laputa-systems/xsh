##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_cksum.rs.

use support.uu as uu

# Expected fixtures include NUL-delimited output and are compared as bytes.
proc expected(ctx: TestContext, name: Str) [fs, error] -> Result[Bytes, Error] {
  Ok(fp"{ctx.core_dir}/tests/data/uutils/cksum/{name}".read_bytes()?)
}

# origin: uutils test_cksum::test_tag_after_untagged
test test_uu_cksum_tag_after_untagged { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--tag", "-a=md5", "lorem_ipsum.txt"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "cksum: invalid argument '=md5' for '--algorithm'\nValid arguments are:\n  - 'bsd'\n  - 'sysv'\n  - 'crc'\n  - 'crc32b'\n  - 'md5'\n  - 'sha1'\n  - 'sha224'\n  - 'sha256'\n  - 'sha384'\n  - 'sha512'\n  - 'sha2'\n  - 'sha3'\n  - 'blake2b'\n  - 'sm3'\nTry 'cksum --help' for more information.\n")
}

# origin: uutils test_cksum::test_tag_short
test test_uu_cksum_tag_short { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["-t", "--untagged", "--algorithm=md5", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is(r, "cd724690f7dc61775dfac400a71f2caa  lorem_ipsum.txt\n")
}

# origin: uutils test_cksum::test_unknown_sha
test test_uu_cksum_unknown_sha { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "SHA4 (README.md) = 00000000\n")?
  let r = uu.invoke(s, "cksum", ["-c", "f"])?
  uu.fails(r)
  uu.stderr_contains(r, "f: no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_untagged_algorithm_after_tag
test test_uu_cksum_untagged_algorithm_after_tag { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--tag", "--untagged", "--algorithm=md5", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/md5_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_multiple_files::case_01_sysv
test test_uu_cksum_test_untagged_algorithm_multiple_files_case_01_sysv { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sysv", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sysv_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_multiple_files::case_02_bsd
test test_uu_cksum_test_untagged_algorithm_multiple_files_case_02_bsd { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=bsd", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/bsd_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_multiple_files::case_03_crc
test test_uu_cksum_test_untagged_algorithm_multiple_files_case_03_crc { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=crc", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/crc_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_multiple_files::case_04_md5
test test_uu_cksum_test_untagged_algorithm_multiple_files_case_04_md5 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=md5", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/md5_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_multiple_files::case_05_sha1
test test_uu_cksum_test_untagged_algorithm_multiple_files_case_05_sha1 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha1", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha1_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_multiple_files::case_06_sha224
test test_uu_cksum_test_untagged_algorithm_multiple_files_case_06_sha224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha224", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha224_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_multiple_files::case_07_sha256
test test_uu_cksum_test_untagged_algorithm_multiple_files_case_07_sha256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha256", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha256_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_multiple_files::case_08_sha384
test test_uu_cksum_test_untagged_algorithm_multiple_files_case_08_sha384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha384", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha384_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_multiple_files::case_09_sha512
test test_uu_cksum_test_untagged_algorithm_multiple_files_case_09_sha512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha512", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha512_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_multiple_files::case_10_blake2b
test test_uu_cksum_test_untagged_algorithm_multiple_files_case_10_blake2b { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=blake2b", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/blake2b_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_multiple_files::case_12_sm3
test test_uu_cksum_test_untagged_algorithm_multiple_files_case_12_sm3 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sm3", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sm3_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_single_file::case_01_sysv
test test_uu_cksum_test_untagged_algorithm_single_file_case_01_sysv { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sysv", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sysv_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_single_file::case_02_bsd
test test_uu_cksum_test_untagged_algorithm_single_file_case_02_bsd { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=bsd", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/bsd_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_single_file::case_03_crc
test test_uu_cksum_test_untagged_algorithm_single_file_case_03_crc { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=crc", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/crc_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_single_file::case_04_md5
test test_uu_cksum_test_untagged_algorithm_single_file_case_04_md5 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=md5", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/md5_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_single_file::case_05_sha1
test test_uu_cksum_test_untagged_algorithm_single_file_case_05_sha1 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha1", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha1_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_single_file::case_06_sha224
test test_uu_cksum_test_untagged_algorithm_single_file_case_06_sha224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha224", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha224_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_single_file::case_07_sha256
test test_uu_cksum_test_untagged_algorithm_single_file_case_07_sha256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha256", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha256_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_single_file::case_08_sha384
test test_uu_cksum_test_untagged_algorithm_single_file_case_08_sha384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha384", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha384_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_single_file::case_09_sha512
test test_uu_cksum_test_untagged_algorithm_single_file_case_09_sha512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha512", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha512_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_single_file::case_10_blake2b
test test_uu_cksum_test_untagged_algorithm_single_file_case_10_blake2b { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=blake2b", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/blake2b_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_single_file::case_12_sm3
test test_uu_cksum_test_untagged_algorithm_single_file_case_12_sm3 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sm3", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sm3_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_stdin::case_01_sysv
test test_uu_cksum_test_untagged_algorithm_stdin_case_01_sysv { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sysv"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sysv_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_stdin::case_02_bsd
test test_uu_cksum_test_untagged_algorithm_stdin_case_02_bsd { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=bsd"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/bsd_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_stdin::case_03_crc
test test_uu_cksum_test_untagged_algorithm_stdin_case_03_crc { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=crc"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/crc_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_stdin::case_04_md5
test test_uu_cksum_test_untagged_algorithm_stdin_case_04_md5 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=md5"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/md5_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_stdin::case_05_sha1
test test_uu_cksum_test_untagged_algorithm_stdin_case_05_sha1 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha1"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha1_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_stdin::case_06_sha224
test test_uu_cksum_test_untagged_algorithm_stdin_case_06_sha224 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha224"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha224_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_stdin::case_07_sha256
test test_uu_cksum_test_untagged_algorithm_stdin_case_07_sha256 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha256"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha256_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_stdin::case_08_sha384
test test_uu_cksum_test_untagged_algorithm_stdin_case_08_sha384 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha384"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha384_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_stdin::case_09_sha512
test test_uu_cksum_test_untagged_algorithm_stdin_case_09_sha512 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sha512"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha512_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_stdin::case_10_blake2b
test test_uu_cksum_test_untagged_algorithm_stdin_case_10_blake2b { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=blake2b"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/blake2b_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_algorithm_stdin::case_12_sm3
test test_uu_cksum_test_untagged_algorithm_stdin_case_12_sm3 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "--algorithm=sm3"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sm3_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_multiple_files
test test_uu_cksum_untagged_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/crc_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_multiple_files::algo_1___sha2__::len_1_224
test test_uu_cksum_test_untagged_sha_multiple_files_algo_1___sha2___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=224", "--untagged", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha224_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_multiple_files::algo_1___sha2__::len_2_256
test test_uu_cksum_test_untagged_sha_multiple_files_algo_1___sha2___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=256", "--untagged", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha256_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_multiple_files::algo_1___sha2__::len_3_384
test test_uu_cksum_test_untagged_sha_multiple_files_algo_1___sha2___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=384", "--untagged", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha384_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_multiple_files::algo_1___sha2__::len_4_512
test test_uu_cksum_test_untagged_sha_multiple_files_algo_1___sha2___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=512", "--untagged", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha512_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_multiple_files::algo_2___sha3__::len_1_224
test test_uu_cksum_test_untagged_sha_multiple_files_algo_2___sha3___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=224", "--untagged", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha3_224_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_multiple_files::algo_2___sha3__::len_2_256
test test_uu_cksum_test_untagged_sha_multiple_files_algo_2___sha3___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=256", "--untagged", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha3_256_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_multiple_files::algo_2___sha3__::len_3_384
test test_uu_cksum_test_untagged_sha_multiple_files_algo_2___sha3___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=384", "--untagged", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha3_384_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_multiple_files::algo_2___sha3__::len_4_512
test test_uu_cksum_test_untagged_sha_multiple_files_algo_2___sha3___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=512", "--untagged", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha3_512_multiple_files.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_single_file::algo_1___sha2__::len_1_224
test test_uu_cksum_test_untagged_sha_single_file_algo_1___sha2___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=224", "--untagged", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha224_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_single_file::algo_1___sha2__::len_2_256
test test_uu_cksum_test_untagged_sha_single_file_algo_1___sha2___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=256", "--untagged", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha256_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_single_file::algo_1___sha2__::len_3_384
test test_uu_cksum_test_untagged_sha_single_file_algo_1___sha2___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=384", "--untagged", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha384_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_single_file::algo_1___sha2__::len_4_512
test test_uu_cksum_test_untagged_sha_single_file_algo_1___sha2___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=512", "--untagged", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha512_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_single_file::algo_2___sha3__::len_1_224
test test_uu_cksum_test_untagged_sha_single_file_algo_2___sha3___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=224", "--untagged", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha3_224_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_single_file::algo_2___sha3__::len_2_256
test test_uu_cksum_test_untagged_sha_single_file_algo_2___sha3___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=256", "--untagged", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha3_256_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_single_file::algo_2___sha3__::len_3_384
test test_uu_cksum_test_untagged_sha_single_file_algo_2___sha3___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=384", "--untagged", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha3_384_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_single_file::algo_2___sha3__::len_4_512
test test_uu_cksum_test_untagged_sha_single_file_algo_2___sha3___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=512", "--untagged", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha3_512_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_stdin::algo_1___sha2__::len_1_224
test test_uu_cksum_test_untagged_sha_stdin_algo_1___sha2___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=224", "--untagged"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha224_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_stdin::algo_1___sha2__::len_2_256
test test_uu_cksum_test_untagged_sha_stdin_algo_1___sha2___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=256", "--untagged"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha256_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_stdin::algo_1___sha2__::len_3_384
test test_uu_cksum_test_untagged_sha_stdin_algo_1___sha2___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=384", "--untagged"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha384_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_stdin::algo_1___sha2__::len_4_512
test test_uu_cksum_test_untagged_sha_stdin_algo_1___sha2___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=512", "--untagged"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha512_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_stdin::algo_2___sha3__::len_1_224
test test_uu_cksum_test_untagged_sha_stdin_algo_2___sha3___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=224", "--untagged"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha3_224_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_stdin::algo_2___sha3__::len_2_256
test test_uu_cksum_test_untagged_sha_stdin_algo_2___sha3___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=256", "--untagged"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha3_256_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_stdin::algo_2___sha3__::len_3_384
test test_uu_cksum_test_untagged_sha_stdin_algo_2___sha3___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=384", "--untagged"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha3_384_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_sha_stdin::algo_2___sha3__::len_4_512
test test_uu_cksum_test_untagged_sha_stdin_algo_2___sha3___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=512", "--untagged"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/sha3_512_stdin.expected")?)
}

# origin: uutils test_cksum::test_untagged_single_file
test test_uu_cksum_untagged_single_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/crc_single_file.expected")?)
}

# origin: uutils test_cksum::test_untagged_stdin
test test_uu_cksum_untagged_stdin { |ctx|
  let s = uu.scene(ctx)?
  let input = expected(ctx, "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--untagged"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "untagged/crc_stdin.expected")?)
}

# origin: uutils test_cksum::test_zero_multiple_file
test test_uu_cksum_zero_multiple_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["-z", "alice_in_wonderland.txt", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "zero_multiple_file.expected")?)
}

# origin: uutils test_cksum::test_zero_single_file
test test_uu_cksum_zero_single_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--zero", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected(ctx, "zero_single_file.expected")?)
}
