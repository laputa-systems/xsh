##! Native ports of the uutils sha384sum integration tests.

use support.uu as uu

# origin: uutils test_sha384sum::sha384::test_check
test test_uu_sha384sum_sha384_check { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha384sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha384sum", "sha384.checkfile", "sha384.checkfile")?
  let r = uu.invoke(s, "sha384sum", ["--check", "sha384.checkfile"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "input.txt: OK\n")
}

# origin: uutils test_sha384sum::sha384::test_missing_file
test test_uu_sha384sum_sha384_missing_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "file1\n")?
  uu.write(s, "c", "file3\n")?
  let r = uu.invoke(s, "sha384sum", ["a", "b", "c"])?
  uu.fails(r)
  uu.stdout_contains(r, "a\n")
  uu.stdout_contains(r, "c\n")
  uu.stderr_contains(r, "b: No such file or directory")
}

# origin: uutils test_sha384sum::sha384::test_single_file
test test_uu_sha384sum_sha384_single_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha384sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha384sum", "sha384.expected", "sha384.expected")?
  let r = uu.invoke(s, "sha384sum", ["input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == uu.read_text(s, "sha384.expected")?
}

# origin: uutils test_sha384sum::sha384::test_stdin
test test_uu_sha384sum_sha384_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha384sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha384sum", "sha384.expected", "sha384.expected")?
  let r = uu.invoke(s, "sha384sum", [], stdin: uu.read(s, "input.txt")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == uu.read_text(s, "sha384.expected")?
}

# origin: uutils test_sha384sum::sha384::test_stdin_with_dash_directory
test test_uu_sha384sum_sha384_stdin_with_dash_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha384sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha384sum", "sha384.expected", "sha384.expected")?
  uu.mkdir(s, "-")?
  let r = uu.invoke(s, "sha384sum", [], stdin: uu.read(s, "input.txt")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == uu.read_text(s, "sha384.expected")?
}

# origin: uutils test_sha384sum::sha384::test_zero
test test_uu_sha384sum_sha384_zero { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha384sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha384sum", "sha384.expected", "sha384.expected")?
  let r = uu.invoke(s, "sha384sum", ["--zero", "input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == uu.read_text(s, "sha384.expected")?
}

# origin: uutils test_sha384sum::test_conflicting_arg
test test_uu_sha384sum_conflicting_arg { |ctx|
  let s = uu.scene(ctx)?
  let checking = uu.invoke(s, "sha384sum", ["--tag", "--check"])?
  uu.fails_with_code(checking, 1)
  let textmode = uu.invoke(s, "sha384sum", ["--tag", "--text"])?
  uu.fails_with_code(textmode, 1)
}

# origin: uutils test_sha384sum::test_invalid_arg
test test_uu_sha384sum_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sha384sum", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

