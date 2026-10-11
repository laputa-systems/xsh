##! Transcribed from the uutils coreutils integration tests for sha512sum.

use support.uu as uu

proc checksum_scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  for name in ["input.txt", "sha512.expected", "sha512.checkfile"] {
    uu.fixture(s, "sha512sum", name, name)?
  }
  Ok(s)
}

# origin: uutils test_sha512sum::sha512::test_check
test test_uu_sha512sum_sha512_check { |ctx|
  let s = checksum_scene(ctx)?
  let r = uu.invoke(s, "sha512sum", ["--check", "sha512.checkfile"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "input.txt: OK\n")
}

# origin: uutils test_sha512sum::sha512::test_missing_file
test test_uu_sha512sum_sha512_missing_file { |ctx|
  let s = checksum_scene(ctx)?
  uu.write(s, "a", "file1\n")?
  uu.write(s, "c", "file3\n")?
  let r = uu.invoke(s, "sha512sum", ["a", "b", "c"])?
  uu.fails(r)
  uu.stdout_contains(r, "a\n")
  uu.stdout_contains(r, "c\n")
  uu.stderr_contains(r, "b: No such file or directory")
}

# origin: uutils test_sha512sum::sha512::test_single_file
test test_uu_sha512sum_sha512_single_file { |ctx|
  let s = checksum_scene(ctx)?
  let expected = uu.read_text(s, "sha512.expected")?
  let r = uu.invoke(s, "sha512sum", ["input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == expected
}

# origin: uutils test_sha512sum::sha512::test_stdin
test test_uu_sha512sum_sha512_stdin { |ctx|
  let s = checksum_scene(ctx)?
  let expected = uu.read_text(s, "sha512.expected")?
  let r = uu.invoke(s, "sha512sum", [], stdin: uu.read(s, "input.txt")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == expected
}

# origin: uutils test_sha512sum::sha512::test_stdin_with_dash_directory
test test_uu_sha512sum_sha512_stdin_with_dash_directory { |ctx|
  let s = checksum_scene(ctx)?
  uu.mkdir(s, "-")?
  let expected = uu.read_text(s, "sha512.expected")?
  let r = uu.invoke(s, "sha512sum", [], stdin: uu.read(s, "input.txt")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == expected
}

# origin: uutils test_sha512sum::sha512::test_zero
test test_uu_sha512sum_sha512_zero { |ctx|
  let s = checksum_scene(ctx)?
  let expected = uu.read_text(s, "sha512.expected")?
  let r = uu.invoke(s, "sha512sum", ["--zero", "input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == expected
}

# origin: uutils test_sha512sum::test_conflicting_arg
test test_uu_sha512sum_conflicting_arg { |ctx|
  let s = checksum_scene(ctx)?
  let check = uu.invoke(s, "sha512sum", ["--tag", "--check"])?
  uu.fails_with_code(check, 1)
  let text = uu.invoke(s, "sha512sum", ["--tag", "--text"])?
  uu.fails_with_code(text, 1)
}

# origin: uutils test_sha512sum::test_invalid_arg
test test_uu_sha512sum_invalid_arg { |ctx|
  let s = checksum_scene(ctx)?
  let r = uu.invoke(s, "sha512sum", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}
