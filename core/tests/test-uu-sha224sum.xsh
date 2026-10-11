##! Transcribed SHA224 checksum tests from the uutils coreutils suite.

use support.uu as uu

# origin: uutils test_sha224sum::sha224::test_single_file
test test_uu_sha224sum_sha224_single_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha224sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha224sum", "sha224.expected", "sha224.expected")?
  let r = uu.invoke(s, "sha224sum", ["input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let digest = r.stdout.utf8()?.split(" ")[0]
  assert digest == uu.read_text(s, "sha224.expected")?
}

# origin: uutils test_sha224sum::sha224::test_stdin
test test_uu_sha224sum_sha224_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha224sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha224sum", "sha224.expected", "sha224.expected")?
  let r = uu.invoke(s, "sha224sum", [], stdin: uu.read(s, "input.txt")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  let digest = r.stdout.utf8()?.split(" ")[0]
  assert digest == uu.read_text(s, "sha224.expected")?
}

# origin: uutils test_sha224sum::sha224::test_stdin_with_dash_directory
test test_uu_sha224sum_sha224_stdin_with_dash_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha224sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha224sum", "sha224.expected", "sha224.expected")?
  uu.mkdir(s, "-")?
  let r = uu.invoke(s, "sha224sum", [], stdin: uu.read(s, "input.txt")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  let digest = r.stdout.utf8()?.split(" ")[0]
  assert digest == uu.read_text(s, "sha224.expected")?
}

# origin: uutils test_sha224sum::sha224::test_zero
test test_uu_sha224sum_sha224_zero { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha224sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha224sum", "sha224.expected", "sha224.expected")?
  let r = uu.invoke(s, "sha224sum", ["--zero", "input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let digest = r.stdout.utf8()?.split(" ")[0]
  assert digest == uu.read_text(s, "sha224.expected")?
}

# origin: uutils test_sha224sum::sha224::test_check
test test_uu_sha224sum_sha224_check { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha224sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha224sum", "sha224.checkfile", "sha224.checkfile")?
  let r = uu.invoke(s, "sha224sum", ["--check", "sha224.checkfile"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "input.txt: OK\n")
}

# origin: uutils test_sha224sum::sha224::test_missing_file
test test_uu_sha224sum_sha224_missing_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "file1\n")?
  uu.write(s, "c", "file3\n")?
  let r = uu.invoke(s, "sha224sum", ["a", "b", "c"])?
  uu.fails(r)
  uu.stdout_contains(r, "a\n")
  uu.stdout_contains(r, "c\n")
  uu.stderr_contains(r, "b: No such file or directory")
}

# origin: uutils test_sha224sum::test_invalid_arg
test test_uu_sha224sum_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sha224sum", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_sha224sum::test_conflicting_arg
test test_uu_sha224sum_conflicting_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sha224sum", ["--tag", "--check"])?
  uu.fails_with_code(r, 1)
}
