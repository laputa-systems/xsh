##! Transcribed checksum compatibility assertions from uutils.

use support.uu as uu

# origin: uutils test_cksum::test_locale_aware_error_filename_escaping
test test_uu_cksum_locale_aware_error_filename_escaping { |ctx|
  let s = uu.scene(ctx)?
  for locale in ["en_US.UTF-8", "C"] {
    let r = uu.invoke(s, "cksum", ["file_Ã"], vars: {LC_ALL: locale})?
    uu.fails_with_code(r, 1)
    uu.stderr_is(r, if locale == "C" { "cksum: 'file_'$'\\303\\203': No such file or directory\n" } else { "cksum: file_Ã: No such file or directory\n" })
  }
  uu.mkdir(s, "dir_Ã")?
  let directory = uu.invoke(s, "cksum", ["dir_Ã"], vars: {LC_ALL: "C"})?
  uu.fails_with_code(directory, 1)
  uu.stderr_is(directory, "cksum: 'dir_'$'\\303\\203': Is a directory\n")
  for locale in ["C", "en_US.UTF-8"] {
    let r = uu.invoke(s, "cksum", ["file_x(y"], vars: {LC_ALL: locale})?
    uu.fails_with_code(r, 1)
    uu.stderr_is(r, "cksum: 'file_x(y': No such file or directory\n")
  }
}

# origin: uutils test_cksum::test_md5_bits
test test_uu_cksum_md5_bits { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "MD5-65536 (README.md) = e5773576fc75ff0f8eba14f61587ae28\n")?
  let r = uu.invoke(s, "cksum", ["-c", "f"])?
  uu.fails(r)
  uu.stderr_contains(r, "f: no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_multiple_files
test test_uu_cksum_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/crc_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_nonexisting_file
test test_uu_cksum_nonexisting_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cksum", ["asdf"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_contains(r, "cksum: asdf: No such file or directory")
}

# origin: uutils test_cksum::test_nonexisting_file_out
test test_uu_cksum_nonexisting_file_out { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "MD5 (nonexistent) = e5773576fc75ff0f8eba14f61587ae28\n")?
  let r = uu.invoke(s, "cksum", ["-c", "f"])?
  uu.fails(r)
  uu.stdout_contains(r, "nonexistent: FAILED open or read")
  uu.stderr_contains(r, "cksum: nonexistent: No such file or directory")
}

# origin: uutils test_cksum::test_one_nonexisting_file
test test_uu_cksum_one_nonexisting_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "abc.txt")?
  uu.touch(s, "xyz.txt")?
  let r = uu.invoke(s, "cksum", ["abc.txt", "asdf.txt", "xyz.txt"])?
  uu.fails(r)
  assert "4294967295 0 xyz.txt" in r.stdout.utf8()?.lines()
  assert "4294967295 0 abc.txt" in r.stdout.utf8()?.lines()
  uu.stderr_contains(r, "asdf.txt: No such file or directory")
}

# origin: uutils test_cksum::test_raw_multiple_files
test test_uu_cksum_raw_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--raw", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_contains(r, "cksum: the --raw option is not supported with multiple files")
}

# origin: uutils test_cksum::test_raw_single_file::case_01_sysv
test test_uu_cksum_test_raw_single_file_case_01_sysv { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--raw", "lorem_ipsum.txt", "--algorithm=sysv"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/raw/sysv_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_raw_single_file::case_02_bsd
test test_uu_cksum_test_raw_single_file_case_02_bsd { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--raw", "lorem_ipsum.txt", "--algorithm=bsd"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/raw/bsd_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_raw_single_file::case_03_crc
test test_uu_cksum_test_raw_single_file_case_03_crc { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--raw", "lorem_ipsum.txt", "--algorithm=crc"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/raw/crc_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_raw_single_file::case_04_md5
test test_uu_cksum_test_raw_single_file_case_04_md5 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--raw", "lorem_ipsum.txt", "--algorithm=md5"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/raw/md5_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_raw_single_file::case_05_sha1
test test_uu_cksum_test_raw_single_file_case_05_sha1 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--raw", "lorem_ipsum.txt", "--algorithm=sha1"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/raw/sha1_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_raw_single_file::case_06_sha224
test test_uu_cksum_test_raw_single_file_case_06_sha224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--raw", "lorem_ipsum.txt", "--algorithm=sha224"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/raw/sha224_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_raw_single_file::case_07_sha256
test test_uu_cksum_test_raw_single_file_case_07_sha256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--raw", "lorem_ipsum.txt", "--algorithm=sha256"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/raw/sha256_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_raw_single_file::case_08_sha384
test test_uu_cksum_test_raw_single_file_case_08_sha384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--raw", "lorem_ipsum.txt", "--algorithm=sha384"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/raw/sha384_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_raw_single_file::case_09_sha512
test test_uu_cksum_test_raw_single_file_case_09_sha512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--raw", "lorem_ipsum.txt", "--algorithm=sha512"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/raw/sha512_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_raw_single_file::case_10_blake2b
test test_uu_cksum_test_raw_single_file_case_10_blake2b { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--raw", "lorem_ipsum.txt", "--algorithm=blake2b"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/raw/blake2b_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_raw_single_file::case_12_sm3
test test_uu_cksum_test_raw_single_file_case_12_sm3 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--raw", "lorem_ipsum.txt", "--algorithm=sm3"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/raw/sm3_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_repeated_flags
test test_uu_cksum_repeated_flags { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["-a", "sha1", "--algo=sha256", "-a=md5", "lorem_ipsum.txt"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_contains(r, "invalid argument '=md5' for '--algorithm'")
}

# origin: uutils test_cksum::test_reset_binary
test test_uu_cksum_reset_binary { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r = uu.invoke(s, "cksum", ["--binary", "--tag", "--untagged", "--algorithm=md5"].extend([uu.at(s, "f").display()]))?
  uu.succeeds(r)
  uu.stdout_contains(r, "d41d8cd98f00b204e9800998ecf8427e *")
}

# origin: uutils test_cksum::test_reset_binary_but_set
test test_uu_cksum_reset_binary_but_set { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r = uu.invoke(s, "cksum", ["--binary", "--tag", "--untagged", "--binary", "--algorithm=md5"].extend([uu.at(s, "f").display()]))?
  uu.succeeds(r)
  uu.stdout_contains(r, "d41d8cd98f00b204e9800998ecf8427e *")
}

# origin: uutils test_cksum::test_several_files_error_mgmt
test test_uu_cksum_several_files_error_mgmt { |ctx|
  let s = uu.scene(ctx)?
  let missing = uu.invoke(s, "cksum", ["--check", "empty", "incorrect"])?
  uu.fails(missing)
  uu.stderr_contains(missing, "empty: No such file ")
  uu.stderr_contains(missing, "incorrect: No such file ")
  uu.touch(s, "empty")?
  uu.touch(s, "incorrect")?
  let r = uu.invoke(s, "cksum", ["--check", "empty", "incorrect"])?
  uu.fails(r)
  uu.stderr_contains(r, "empty: no properly ")
  uu.stderr_contains(r, "incorrect: no properly ")
}

# origin: uutils test_cksum::test_sha_length_invalid
test test_uu_cksum_sha_length_invalid { |ctx|
  let s = uu.scene(ctx)?
  for algo in ["sha2", "sha3"] {
    for length in ["0", "00", "13", "56", "99999999999999999999999999", "512x", "x512", "512x512"] {
      for check in [false, true] {
        let args = ["--algorithm", algo, "--length", length, "/dev/null"]
        let r = uu.invoke(s, "cksum", if check { args.extend(["--check"]) } else { args })?
        uu.fails_with_code(r, 1)
        uu.no_stdout(r)
        uu.stderr_contains(r, f"invalid length: '{length}'")
        if length in ["0", "00", "13", "56", "99999999999999999999999999"] {
          let label = if algo == "sha2" { "SHA2" } else { "SHA3" }
          uu.stderr_contains(r, f"digest length for '{label}' must be 224, 256, 384, or 512")
        }
      }
    }
  }
}

# origin: uutils test_cksum::test_sha_missing_length
test test_uu_cksum_sha_missing_length { |ctx|
  let s = uu.scene(ctx)?
  for algo in ["sha2", "sha3"] {
    let r = uu.invoke(s, "cksum", ["--algorithm", algo, "lorem_ipsum.txt"])?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
    uu.stderr_contains(r, f"--algorithm={algo} requires specifying --length 224, 256, 384, or 512")
  }
}

# origin: uutils test_cksum::test_sha_multiple_files::algo_1___sha2__::len_1_224
test test_uu_cksum_test_sha_multiple_files_algo_1___sha2___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=224", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha224_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_multiple_files::algo_1___sha2__::len_2_256
test test_uu_cksum_test_sha_multiple_files_algo_1___sha2___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=256", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha256_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_multiple_files::algo_1___sha2__::len_3_384
test test_uu_cksum_test_sha_multiple_files_algo_1___sha2___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=384", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha384_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_multiple_files::algo_1___sha2__::len_4_512
test test_uu_cksum_test_sha_multiple_files_algo_1___sha2___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=512", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha512_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_multiple_files::algo_2___sha3__::len_1_224
test test_uu_cksum_test_sha_multiple_files_algo_2___sha3___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=224", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha3_224_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_multiple_files::algo_2___sha3__::len_2_256
test test_uu_cksum_test_sha_multiple_files_algo_2___sha3___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=256", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha3_256_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_multiple_files::algo_2___sha3__::len_3_384
test test_uu_cksum_test_sha_multiple_files_algo_2___sha3___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=384", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha3_384_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_multiple_files::algo_2___sha3__::len_4_512
test test_uu_cksum_test_sha_multiple_files_algo_2___sha3___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=512", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha3_512_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_single_file::algo_1___sha2__::len_1_224
test test_uu_cksum_test_sha_single_file_algo_1___sha2___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=224", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha224_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_single_file::algo_1___sha2__::len_2_256
test test_uu_cksum_test_sha_single_file_algo_1___sha2___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=256", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha256_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_single_file::algo_1___sha2__::len_3_384
test test_uu_cksum_test_sha_single_file_algo_1___sha2___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=384", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha384_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_single_file::algo_1___sha2__::len_4_512
test test_uu_cksum_test_sha_single_file_algo_1___sha2___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=512", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha512_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_single_file::algo_2___sha3__::len_1_224
test test_uu_cksum_test_sha_single_file_algo_2___sha3___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=224", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha3_224_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_single_file::algo_2___sha3__::len_2_256
test test_uu_cksum_test_sha_single_file_algo_2___sha3___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=256", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha3_256_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_single_file::algo_2___sha3__::len_3_384
test test_uu_cksum_test_sha_single_file_algo_2___sha3___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=384", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha3_384_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_single_file::algo_2___sha3__::len_4_512
test test_uu_cksum_test_sha_single_file_algo_2___sha3___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=512", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha3_512_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_stdin::algo_1___sha2__::len_1_224
test test_uu_cksum_test_sha_stdin_algo_1___sha2___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=224"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha224_stdin.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_stdin::algo_1___sha2__::len_2_256
test test_uu_cksum_test_sha_stdin_algo_1___sha2___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=256"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha256_stdin.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_stdin::algo_1___sha2__::len_3_384
test test_uu_cksum_test_sha_stdin_algo_1___sha2___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=384"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha384_stdin.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_stdin::algo_1___sha2__::len_4_512
test test_uu_cksum_test_sha_stdin_algo_1___sha2___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=512"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha512_stdin.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_stdin::algo_2___sha3__::len_1_224
test test_uu_cksum_test_sha_stdin_algo_2___sha3___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=224"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha3_224_stdin.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_stdin::algo_2___sha3__::len_2_256
test test_uu_cksum_test_sha_stdin_algo_2___sha3___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=256"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha3_256_stdin.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_stdin::algo_2___sha3__::len_3_384
test test_uu_cksum_test_sha_stdin_algo_2___sha3___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=384"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha3_384_stdin.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_sha_stdin::algo_2___sha3__::len_4_512
test test_uu_cksum_test_sha_stdin_algo_2___sha3___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["--algorithm=sha3", "--length=512"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha3_512_stdin.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_shake_extremely_large_length_does_not_abort
test test_uu_cksum_shake_extremely_large_length_does_not_abort { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cksum", ["--algorithm", "shake128", "--length", "10011111117721172727"])?
  uu.fails(r)
}

# origin: uutils test_cksum::test_single_file
test test_uu_cksum_single_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", ["lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/crc_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_stdin
test test_uu_cksum_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "cksum", [], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/crc_stdin.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_stdin_larger_than_128_bytes
test test_uu_cksum_stdin_larger_than_128_bytes { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "larger_than_2056_bytes.txt", "larger_than_2056_bytes.txt")?
  let r = uu.invoke(s, "cksum", ["larger_than_2056_bytes.txt"])?
  uu.succeeds(r)
  let words = r.stdout.utf8()?.split(" ")
  assert words[0] == "945881979"
  assert words[1] == "2058"
}

# origin: uutils test_cksum::test_stdin_with_dash_directory
test test_uu_cksum_stdin_with_dash_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.mkdir(s, "-")?
  let r = uu.invoke(s, "cksum", [], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/crc_stdin.expected".read_bytes()?)
}

