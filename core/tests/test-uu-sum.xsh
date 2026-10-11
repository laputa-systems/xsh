##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_sum.rs.

use support.uu as uu

# origin: uutils test_sum::test_bsd_multiple_files
test test_uu_sum_bsd_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "sum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "sum", ["lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/sum/bsd_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_sum::test_bsd_single_file
test test_uu_sum_bsd_single_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "sum", ["lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/sum/bsd_single_file.expected".read_bytes()?)
}

# origin: uutils test_sum::test_bsd_stdin
test test_uu_sum_bsd_stdin { |ctx|
  let s = uu.scene(ctx)?
  let input = fp"{ctx.core_dir}/tests/data/uutils/sum/lorem_ipsum.txt".read_bytes()?
  let r = uu.invoke(s, "sum", [], stdin: input)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/sum/bsd_stdin.expected".read_bytes()?)
}

# origin: uutils test_sum::test_sysv_multiple_files
test test_uu_sum_sysv_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "sum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  let r = uu.invoke(s, "sum", ["-s", "lorem_ipsum.txt", "alice_in_wonderland.txt"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/sum/sysv_multiple_files.expected".read_bytes()?)
}

# origin: uutils test_sum::test_sysv_single_file
test test_uu_sum_sysv_single_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "sum", ["-s", "lorem_ipsum.txt"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/sum/sysv_single_file.expected".read_bytes()?)
}

# origin: uutils test_sum::test_sysv_stdin
test test_uu_sum_sysv_stdin { |ctx|
  let s = uu.scene(ctx)?
  let input = fp"{ctx.core_dir}/tests/data/uutils/sum/lorem_ipsum.txt".read_bytes()?
  let r = uu.invoke(s, "sum", ["-s"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/sum/sysv_stdin.expected".read_bytes()?)
}

# origin: uutils test_sum::test_filename_ends_with_slash
test test_uu_sum_filename_ends_with_slash { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "sum", ["a/"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "sum: a/: Not a directory\n")
}

# origin: uutils test_sum::test_filename_proc_self_mem
test test_uu_sum_filename_proc_self_mem { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sum", ["/proc/self/mem"], timeout: 5s)?
  uu.fails_with_code(r, 1)
  assert r.stderr == b"sum: /proc/self/mem: Input/output error\n" or r.stderr == b"sum: /proc/self/mem: I/O error\n", "read diagnostic"
}

# origin: uutils test_sum::test_invalid_arg
test test_uu_sum_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sum", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_sum::test_invalid_file
test test_uu_sum_invalid_file { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  let r = uu.invoke(s, "sum", ["a"])?
  uu.fails(r)
  uu.stderr_is(r, "sum: a: Is a directory\n")
}

# origin: uutils test_sum::test_invalid_file_does_not_stop_other_files
test test_uu_sum_invalid_file_does_not_stop_other_files { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.write(s, "f", "hello\n")?
  let r = uu.invoke(s, "sum", ["d", "f"])?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "36979     1 f\n")
  uu.stderr_is(r, "sum: d: Is a directory\n")
}

# origin: uutils test_sum::test_invalid_metadata
test test_uu_sum_invalid_metadata { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sum", ["b"])?
  uu.fails(r)
  uu.stderr_is(r, "sum: b: No such file or directory\n")
}

# origin: uutils test_sum::test_sum_non_utf8_paths
test test_uu_sum_sum_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  uu.at_bytes(s, b"\xff\xfe")?.write(b"test content")?
  let r = uu.invoke_paths(s, "sum", [Path.parse_bytes(b"\xff\xfe")?])?
  uu.succeeds(r)
}
