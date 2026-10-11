##! Transcribed from the MIT-licensed uutils cksum integration tests.

use support.uu as uu

# origin: uutils test_cksum::test_algorithm_single_file::case_09_sha512
test test_uu_cksum_test_algorithm_single_file_case_09_sha512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=sha512", "lorem_ipsum.txt"])?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=sha512' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha512_single_file.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_single_file::case_10_blake2b
test test_uu_cksum_test_algorithm_single_file_case_10_blake2b { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=blake2b", "lorem_ipsum.txt"])?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=blake2b' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/blake2b_single_file.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_single_file::case_12_sm3
test test_uu_cksum_test_algorithm_single_file_case_12_sm3 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=sm3", "lorem_ipsum.txt"])?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=sm3' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sm3_single_file.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_stdin::case_01_sysv
test test_uu_cksum_test_algorithm_stdin_case_01_sysv { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=sysv"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=sysv' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sysv_stdin.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_stdin::case_02_bsd
test test_uu_cksum_test_algorithm_stdin_case_02_bsd { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=bsd"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=bsd' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/bsd_stdin.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_stdin::case_03_crc
test test_uu_cksum_test_algorithm_stdin_case_03_crc { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=crc"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=crc' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/crc_stdin.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_stdin::case_04_md5
test test_uu_cksum_test_algorithm_stdin_case_04_md5 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=md5"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=md5' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/md5_stdin.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_stdin::case_05_sha1
test test_uu_cksum_test_algorithm_stdin_case_05_sha1 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=sha1"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=sha1' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha1_stdin.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_stdin::case_06_sha224
test test_uu_cksum_test_algorithm_stdin_case_06_sha224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=sha224"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=sha224' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha224_stdin.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_stdin::case_07_sha256
test test_uu_cksum_test_algorithm_stdin_case_07_sha256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=sha256"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=sha256' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha256_stdin.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_stdin::case_08_sha384
test test_uu_cksum_test_algorithm_stdin_case_08_sha384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=sha384"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=sha384' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha384_stdin.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_stdin::case_09_sha512
test test_uu_cksum_test_algorithm_stdin_case_09_sha512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=sha512"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=sha512' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sha512_stdin.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_stdin::case_10_blake2b
test test_uu_cksum_test_algorithm_stdin_case_10_blake2b { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=blake2b"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=blake2b' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/blake2b_stdin.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_algorithm_stdin::case_12_sm3
test test_uu_cksum_test_algorithm_stdin_case_12_sm3 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for option in ["-a", "--algorithm"] {
    let r = uu.invoke(s, "cksum", [f"{option}=sm3"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
    if option == "-a" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_contains(r, "invalid argument '=sm3' for '--algorithm'")
    } else {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cksum/sm3_stdin.expected".read_bytes()?)
    }
  }
}

# origin: uutils test_cksum::test_all_algorithms_fail_on_folder::case_01_sysv
test test_uu_cksum_test_all_algorithms_fail_on_folder_case_01_sysv { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=sysv", "a_folder"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: a_folder: Is a directory")
}

# origin: uutils test_cksum::test_all_algorithms_fail_on_folder::case_02_bsd
test test_uu_cksum_test_all_algorithms_fail_on_folder_case_02_bsd { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=bsd", "a_folder"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: a_folder: Is a directory")
}

# origin: uutils test_cksum::test_all_algorithms_fail_on_folder::case_03_crc
test test_uu_cksum_test_all_algorithms_fail_on_folder_case_03_crc { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=crc", "a_folder"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: a_folder: Is a directory")
}

# origin: uutils test_cksum::test_all_algorithms_fail_on_folder::case_04_md5
test test_uu_cksum_test_all_algorithms_fail_on_folder_case_04_md5 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=md5", "a_folder"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: a_folder: Is a directory")
}

# origin: uutils test_cksum::test_all_algorithms_fail_on_folder::case_05_sha1
test test_uu_cksum_test_all_algorithms_fail_on_folder_case_05_sha1 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=sha1", "a_folder"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: a_folder: Is a directory")
}

# origin: uutils test_cksum::test_all_algorithms_fail_on_folder::case_06_sha224
test test_uu_cksum_test_all_algorithms_fail_on_folder_case_06_sha224 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=sha224", "a_folder"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: a_folder: Is a directory")
}

# origin: uutils test_cksum::test_all_algorithms_fail_on_folder::case_07_sha256
test test_uu_cksum_test_all_algorithms_fail_on_folder_case_07_sha256 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=sha256", "a_folder"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: a_folder: Is a directory")
}

# origin: uutils test_cksum::test_all_algorithms_fail_on_folder::case_08_sha384
test test_uu_cksum_test_all_algorithms_fail_on_folder_case_08_sha384 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=sha384", "a_folder"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: a_folder: Is a directory")
}

# origin: uutils test_cksum::test_all_algorithms_fail_on_folder::case_09_sha512
test test_uu_cksum_test_all_algorithms_fail_on_folder_case_09_sha512 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=sha512", "a_folder"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: a_folder: Is a directory")
}

# origin: uutils test_cksum::test_all_algorithms_fail_on_folder::case_10_blake2b
test test_uu_cksum_test_all_algorithms_fail_on_folder_case_10_blake2b { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=blake2b", "a_folder"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: a_folder: Is a directory")
}

# origin: uutils test_cksum::test_all_algorithms_fail_on_folder::case_12_sm3
test test_uu_cksum_test_all_algorithms_fail_on_folder_case_12_sm3 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=sm3", "a_folder"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: a_folder: Is a directory")
}

# origin: uutils test_cksum::test_arg_overrides_stdin
test test_uu_cksum_arg_overrides_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r1 = uu.invoke(s, "cksum", ["a"], stdin: b"foobarfoobar")?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is(r1, "4294967295 0 a\n")
}

# origin: uutils test_cksum::test_base64_multiple_files
test test_uu_cksum_base64_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "--algorithm=md5", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/base64/md5_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_base64_raw_conflicts
test test_uu_cksum_base64_raw_conflicts { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "--raw", "lorem_ipsum.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "--base64")
  uu.stderr_contains(r1, "are mutually exclusive")
  uu.stderr_contains(r1, "--raw")
}

# origin: uutils test_cksum::test_base64_single_file::case_01_sysv
test test_uu_cksum_test_base64_single_file_case_01_sysv { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "lorem_ipsum.txt", "--algorithm=sysv"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/base64/sysv_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_base64_single_file::case_02_bsd
test test_uu_cksum_test_base64_single_file_case_02_bsd { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "lorem_ipsum.txt", "--algorithm=bsd"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/base64/bsd_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_base64_single_file::case_03_crc
test test_uu_cksum_test_base64_single_file_case_03_crc { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "lorem_ipsum.txt", "--algorithm=crc"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/base64/crc_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_base64_single_file::case_04_md5
test test_uu_cksum_test_base64_single_file_case_04_md5 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "lorem_ipsum.txt", "--algorithm=md5"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/base64/md5_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_base64_single_file::case_05_sha1
test test_uu_cksum_test_base64_single_file_case_05_sha1 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "lorem_ipsum.txt", "--algorithm=sha1"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/base64/sha1_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_base64_single_file::case_06_sha224
test test_uu_cksum_test_base64_single_file_case_06_sha224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "lorem_ipsum.txt", "--algorithm=sha224"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/base64/sha224_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_base64_single_file::case_07_sha256
test test_uu_cksum_test_base64_single_file_case_07_sha256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "lorem_ipsum.txt", "--algorithm=sha256"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/base64/sha256_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_base64_single_file::case_08_sha384
test test_uu_cksum_test_base64_single_file_case_08_sha384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "lorem_ipsum.txt", "--algorithm=sha384"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/base64/sha384_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_base64_single_file::case_09_sha512
test test_uu_cksum_test_base64_single_file_case_09_sha512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "lorem_ipsum.txt", "--algorithm=sha512"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/base64/sha512_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_base64_single_file::case_10_blake2b
test test_uu_cksum_test_base64_single_file_case_10_blake2b { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "lorem_ipsum.txt", "--algorithm=blake2b"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/base64/blake2b_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_base64_single_file::case_12_sm3
test test_uu_cksum_test_base64_single_file_case_12_sm3 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["--base64", "lorem_ipsum.txt", "--algorithm=sm3"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/base64/sm3_single_file.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_binary_file
test test_uu_cksum_binary_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.mkdir(s, "raw")?
  uu.fixture(s, "cksum", "raw/blake2b_single_file.expected", "raw/blake2b_single_file.expected")?
  let r1 = uu.invoke(s, "cksum", ["--untagged", "-b", "--algorithm=md5", uu.at(s, "f").display()])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "d41d8cd98f00b204e9800998ecf8427e *")
  let r2 = uu.invoke(s, "cksum", ["--tag", "--untagged", "--binary", "--algorithm=md5", uu.at(s, "f").display()])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "d41d8cd98f00b204e9800998ecf8427e *")
  let r3 = uu.invoke(s, "cksum", ["--tag", "--untagged", "--binary", "--algorithm=md5", "raw/blake2b_single_file.expected"])?
  uu.succeeds(r3)
  uu.stdout_contains(r3, "7e297c07ed8e053600092f91bdd1dad7 *")
  let r4 = uu.invoke(s, "cksum", ["--tag", "--untagged", "--binary", "--algorithm=md5", "lorem_ipsum.txt"])?
  uu.succeeds(r4)
  uu.stdout_is(r4, "cd724690f7dc61775dfac400a71f2caa *lorem_ipsum.txt\n")
  let r5 = uu.invoke(s, "cksum", ["--untagged", "--binary", "--algorithm=md5", "lorem_ipsum.txt"])?
  uu.succeeds(r5)
  uu.stdout_is(r5, "cd724690f7dc61775dfac400a71f2caa *lorem_ipsum.txt\n")
  let r6 = uu.invoke(s, "cksum", ["--binary", "--untagged", "--algorithm=md5", "lorem_ipsum.txt"])?
  uu.succeeds(r6)
  uu.stdout_is(r6, "cd724690f7dc61775dfac400a71f2caa *lorem_ipsum.txt\n")
  let r7 = uu.invoke(s, "cksum", ["-a", "md5", "--binary", "--untagged", "lorem_ipsum.txt"])?
  uu.succeeds(r7)
  uu.stdout_is(r7, "cd724690f7dc61775dfac400a71f2caa *lorem_ipsum.txt\n")
  let r8 = uu.invoke(s, "cksum", ["-a", "md5", "--binary", "--tag", "--untagged", "lorem_ipsum.txt"])?
  uu.succeeds(r8)
  uu.stdout_is(r8, "cd724690f7dc61775dfac400a71f2caa *lorem_ipsum.txt\n")
}

# origin: uutils test_cksum::test_blake2b_bits
test test_uu_cksum_blake2b_bits { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "BLAKE2b-257 (README.md) = f9a984b70cf9a7549920864860fd1131c9fb6c0552def0b6dcce1d87b4ec4c5d\n")?
  let r1 = uu.invoke(s, "cksum", ["-c", "f"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "f: no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_blake2b_check_digest_too_long
test test_uu_cksum_blake2b_check_digest_too_long { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f1", "content\n")?
  uu.write(s, "sums", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  f1\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-c", "sums"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "sums: no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_blake2b_default_length
test test_uu_cksum_blake2b_default_length { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r1 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l512", "f"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "BLAKE2b (f) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce")
  uu.write(s, "checksum", "BLAKE2b (f) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce")?
  let r2 = uu.invoke(s, "cksum", ["--check", "checksum"])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "f: OK")
  let r3 = uu.invoke(s, "cksum", ["--status", "--check", "checksum"])?
  uu.succeeds(r3)
  uu.no_output(r3)
}

# origin: uutils test_cksum::test_blake2b_length
test test_uu_cksum_blake2b_length { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["--length=16", "--algorithm=blake2b", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "BLAKE2b-16 (lorem_ipsum.txt) = 7e2f\nBLAKE2b-16 (alice_in_wonderland.txt) = a546")
}

# origin: uutils test_cksum::test_blake2b_length_greater_than_512
test test_uu_cksum_blake2b_length_greater_than_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=blake2b", "--length", "513", "lorem_ipsum.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid length: '513'")
  uu.stderr_contains(r1, "maximum digest length for 'BLAKE2b' is 512 bits")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=blake2b", "--length", "1024", "lorem_ipsum.txt"])?
  uu.fails_with_code(r2, 1)
  uu.no_stdout(r2)
  uu.stderr_contains(r2, "invalid length: '1024'")
  uu.stderr_contains(r2, "maximum digest length for 'BLAKE2b' is 512 bits")
  let r3 = uu.invoke(s, "cksum", ["--algorithm=blake2b", "--length", "73786976294838206464", "lorem_ipsum.txt"])?
  uu.fails_with_code(r3, 1)
  uu.no_stdout(r3)
  uu.stderr_contains(r3, "invalid length: '73786976294838206464'")
  uu.stderr_contains(r3, "maximum digest length for 'BLAKE2b' is 512 bits")
}

# origin: uutils test_cksum::test_blake2b_length_invalid
test test_uu_cksum_blake2b_length_invalid { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["--length", "1", "--algorithm=blake2b", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "invalid length: '1'")
  let r2 = uu.invoke(s, "cksum", ["--length", "01", "--algorithm=blake2b", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r2, 1)
  uu.stderr_contains(r2, "invalid length: '01'")
  let r3 = uu.invoke(s, "cksum", ["--length", "", "--algorithm=blake2b", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.fails_with_code(r3, 1)
  uu.stderr_contains(r3, "invalid length: ''")
}

# origin: uutils test_cksum::test_blake2b_length_is_zero
test test_uu_cksum_blake2b_length_is_zero { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["--length=0", "--algorithm=blake2b", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/length_is_zero.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_blake2b_length_nan
test test_uu_cksum_blake2b_length_nan { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["--algorithm=blake2b", "--length", "foo", "lorem_ipsum.txt"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid length: 'foo'")
  let r2 = uu.invoke(s, "cksum", ["--algorithm=blake2b", "--length", "512x", "lorem_ipsum.txt"])?
  uu.fails_with_code(r2, 1)
  uu.no_stdout(r2)
  uu.stderr_contains(r2, "invalid length: '512x'")
  let r3 = uu.invoke(s, "cksum", ["--algorithm=blake2b", "--length", "x512", "lorem_ipsum.txt"])?
  uu.fails_with_code(r3, 1)
  uu.no_stdout(r3)
  uu.stderr_contains(r3, "invalid length: 'x512'")
  let r4 = uu.invoke(s, "cksum", ["--algorithm=blake2b", "--length", "0xff", "lorem_ipsum.txt"])?
  uu.fails_with_code(r4, 1)
  uu.no_stdout(r4)
  uu.stderr_contains(r4, "invalid length: '0xff'")
}

# origin: uutils test_cksum::test_blake2b_length_repeated
test test_uu_cksum_blake2b_length_repeated { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r1 = uu.invoke(s, "cksum", ["--length=10", "--length=123456", "--length=0", "--algorithm=blake2b", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, fp"{ctx.core_dir}/tests/data/uutils/cksum/length_is_zero.expected".read_bytes()?)
}

# origin: uutils test_cksum::test_blake2d_tested_with_sha1
test test_uu_cksum_blake2d_tested_with_sha1 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "BLAKE2b (f) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "sha1", "-c", "f"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "f: no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_bsd_case
test test_uu_cksum_bsd_case { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "bSD (README.md) = 0000\n")?
  let r1 = uu.invoke(s, "cksum", ["-c", "f"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "f: no properly formatted checksum lines found")
  uu.write(s, "f", "BsD (README.md) = 0000\n")?
  let r2 = uu.invoke(s, "cksum", ["-c", "f"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "f: no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_check_algo
test test_uu_cksum_check_algo { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "cksum", ["-a", "bsd", "--check", "lorem_ipsum.txt"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: --check is not supported with --algorithm={bsd,sysv,crc,crc32b}")
  let r2 = uu.invoke(s, "cksum", ["-a", "sysv", "--check", "lorem_ipsum.txt"])?
  uu.fails(r2)
  uu.no_stdout(r2)
  uu.stderr_contains(r2, "cksum: --check is not supported with --algorithm={bsd,sysv,crc,crc32b}")
  let r3 = uu.invoke(s, "cksum", ["-a", "crc", "--check", "lorem_ipsum.txt"])?
  uu.fails(r3)
  uu.no_stdout(r3)
  uu.stderr_contains(r3, "cksum: --check is not supported with --algorithm={bsd,sysv,crc,crc32b}")
  let r4 = uu.invoke(s, "cksum", ["-a", "crc32b", "--check", "lorem_ipsum.txt"])?
  uu.fails(r4)
  uu.no_stdout(r4)
  uu.stderr_contains(r4, "cksum: --check is not supported with --algorithm={bsd,sysv,crc,crc32b}")
}

# origin: uutils test_cksum::test_check_algo_err
test test_uu_cksum_check_algo_err { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r1 = uu.invoke(s, "cksum", ["-a", "sm3", "--check", "f"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: f: no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_check_base64_hashes
test test_uu_cksum_check_base64_hashes { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "empty")?
  uu.write(s, "check", "MD5 (empty) = 1B2M2Y8AsgTpgAmY7PhCfg==\nSHA256 (empty) = 47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=\nBLAKE2b (empty) = eGoC90IBWQPGxv2FJVLScpEvR0DhWEdhiobiF/cfVBnSXhAxr+5YUxOJZESTTrBLkDpoWxRIt1XVb3Aa/pvizg==\n")?
  let r1 = uu.invoke(s, "cksum", ["--check", uu.at(s, "check").display()])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "empty: OK\nempty: OK\nempty: OK\n")
}

# origin: uutils test_cksum::test_check_blake_length_guess
test test_uu_cksum_check_blake_length_guess { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo.dat", "foo")?
  uu.write(s, "foo.sums", "BLAKE2b (foo.dat) = ca002330e69d3e6b84a46a56a6533fd79d51d97a3bb7cad6c2ff43b354185d6dc1e723fb3db4ae0737e120378424c714bb982d9dc5bbd7a0ab318240ddd18f8d")?
  let r1 = uu.invoke(s, "cksum", ["--check", uu.at(s, "foo.sums").display()])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "foo.dat: OK\n")
  uu.write(s, "foo.sums", "BLAKE2b-512 (foo.dat) = ca002330e69d3e6b84a46a56a6533fd79d51d97a3bb7cad6c2ff43b354185d6dc1e723fb3db4ae0737e120378424c714bb982d9dc5bbd7a0ab318240ddd18f8d")?
  let r2 = uu.invoke(s, "cksum", ["--check", uu.at(s, "foo.sums").display()])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "foo.dat: OK\n")
  uu.write(s, "foo.sums", "BLAKE2b-48 (foo.dat) = 171cdfdf84ed")?
  let r3 = uu.invoke(s, "cksum", ["--check", uu.at(s, "foo.sums").display()])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "foo.dat: OK\n")
  uu.write(s, "foo.sums", "BLAKE2b (foo.dat) = 171cdfdf84ed")?
  let r4 = uu.invoke(s, "cksum", ["--check", uu.at(s, "foo.sums").display()])?
  uu.fails(r4)
  uu.stderr_contains(r4, "foo.sums: no properly formatted checksum lines found")
  uu.write(s, "foo.sums", "BLAKE2b-8 (foo.dat) = 171cdfdf84ed")?
  let r5 = uu.invoke(s, "cksum", ["--check", uu.at(s, "foo.sums").display()])?
  uu.fails(r5)
  uu.stderr_contains(r5, "foo.sums: no properly formatted checksum lines found")
  uu.write(s, "foo.sums", "BLAKE2b-520 (/dev/null) = 0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000")?
  let r6 = uu.invoke(s, "cksum", ["--check", uu.at(s, "foo.sums").display()])?
  uu.fails(r6)
  uu.stderr_contains(r6, "foo.sums: no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_check_comment_leading_space
test test_uu_cksum_check_comment_leading_space { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "foo-content\n")?
  uu.write(s, "CHECKSUM-sha1", " # This is a comment\nSHA1 (foo) = 058ab38dd3603703b3a7063cf95dc51a4286b6fe\n")?
  let r1 = uu.invoke(s, "cksum", ["--check", "CHECKSUM-sha1"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "foo: OK")
  uu.stderr_contains(r1, "WARNING: 1 line is improperly formatted")
}

# origin: uutils test_cksum::test_check_comment_line
test test_uu_cksum_check_comment_line { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "foo-content\n")?
  uu.write(s, "CHECKSUM-sha1", "# This is a comment\nSHA1 (foo) = 058ab38dd3603703b3a7063cf95dc51a4286b6fe\n# next comment is empty\n#")?
  let r1 = uu.invoke(s, "cksum", ["--check", "CHECKSUM-sha1"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "foo: OK")
  uu.no_stderr(r1)
}

# origin: uutils test_cksum::test_check_comment_only
test test_uu_cksum_check_comment_only { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "CHECKSUM", "# This is a comment\n")?
  let r1 = uu.invoke(s, "cksum", ["--check", "CHECKSUM"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_check_confusing_base64
test test_uu_cksum_check_confusing_base64 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo.dat", "esq")?
  uu.write(s, "foo.sums", "BLAKE2b-48 (foo.dat) = fc1f97C4")?
  let r1 = uu.invoke(s, "cksum", ["--check", uu.at(s, "foo.sums").display()])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "foo.dat: OK\n")
}

# origin: uutils test_cksum::test_check_crlf_file_hashed_as_raw_bytes
test test_uu_cksum_check_crlf_file_hashed_as_raw_bytes { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "f", b"abc\r\nd\r")?
  uu.write(s, "CHECKSUM", "e5bd2e073913d2cb78174d5be11ab1e9  f\n")?
  let r1 = uu.invoke(s, "cksum", ["-a", "md5", "--check", "CHECKSUM"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "f: OK")
  uu.write(s, "CHECKSUM_BINARY", "e5bd2e073913d2cb78174d5be11ab1e9 *f\n")?
  let r2 = uu.invoke(s, "cksum", ["-a", "md5", "--check", "CHECKSUM_BINARY"])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "f: OK")
}

# origin: uutils test_cksum::test_check_directory_error
test test_uu_cksum_check_directory_error { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.write(s, "f", "BLAKE2b (d) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce\n")?
  let r1 = uu.invoke(s, "cksum", ["--check", uu.at(s, "f").display()])?
  uu.fails(r1)
  uu.stderr_contains(r1, "cksum: d: Is a directory\n")
}

# origin: uutils test_cksum::test_check_error_incorrect_format
test test_uu_cksum_check_error_incorrect_format { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "checksum", "e5773576fc75ff0f8eba14f61587ae28  README.md")?
  let r1 = uu.invoke(s, "cksum", ["-c", "checksum"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "no properly formatted checksum lines found")
}
