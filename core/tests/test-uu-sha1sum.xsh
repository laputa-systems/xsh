##! Transcribed from the uutils coreutils integration tests for sha1sum.

use support.uu as uu

proc checksum_scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  for name in ["input.txt", "sha1.expected", "sha1.checkfile"] {
    uu.fixture(s, "sha1sum", name, name)?
  }
  Ok(s)
}

# origin: uutils test_sha1sum::sha1::test_check
test test_uu_sha1sum_sha1_check { |ctx|
  let s = checksum_scene(ctx)?
  let r = uu.invoke(s, "sha1sum", ["--check", "sha1.checkfile"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "input.txt: OK\n")
}

# origin: uutils test_sha1sum::sha1::test_missing_file
test test_uu_sha1sum_sha1_missing_file { |ctx|
  let s = checksum_scene(ctx)?
  uu.write(s, "a", "file1\n")?
  uu.write(s, "c", "file3\n")?
  let r = uu.invoke(s, "sha1sum", ["a", "b", "c"])?
  uu.fails(r)
  uu.stdout_contains(r, "a\n")
  uu.stdout_contains(r, "c\n")
  uu.stderr_contains(r, "b: No such file or directory")
}

# origin: uutils test_sha1sum::sha1::test_single_file
test test_uu_sha1sum_sha1_single_file { |ctx|
  let s = checksum_scene(ctx)?
  let expected = uu.read_text(s, "sha1.expected")?
  let r = uu.invoke(s, "sha1sum", ["input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == expected
}

# origin: uutils test_sha1sum::sha1::test_stdin
test test_uu_sha1sum_sha1_stdin { |ctx|
  let s = checksum_scene(ctx)?
  let expected = uu.read_text(s, "sha1.expected")?
  let r = uu.invoke(s, "sha1sum", [], stdin: uu.read(s, "input.txt")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == expected
}

# origin: uutils test_sha1sum::sha1::test_stdin_with_dash_directory
test test_uu_sha1sum_sha1_stdin_with_dash_directory { |ctx|
  let s = checksum_scene(ctx)?
  uu.mkdir(s, "-")?
  let expected = uu.read_text(s, "sha1.expected")?
  let r = uu.invoke(s, "sha1sum", [], stdin: uu.read(s, "input.txt")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == expected
}

# origin: uutils test_sha1sum::sha1::test_zero
test test_uu_sha1sum_sha1_zero { |ctx|
  let s = checksum_scene(ctx)?
  let expected = uu.read_text(s, "sha1.expected")?
  let r = uu.invoke(s, "sha1sum", ["--zero", "input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == expected
}

# origin: uutils test_sha1sum::test_check_file_not_found_warning
test test_uu_sha1sum_check_file_not_found_warning { |ctx|
  let s = checksum_scene(ctx)?
  uu.write(s, "testf", "foobar\n")?
  uu.write(s, "testf.sha1", "988881adc9fc3655077dc2d4d757d480b5ea0e11  testf\n")?
  uu.remove(s, "testf")?
  let r = uu.invoke(s, "sha1sum", ["-c", uu.at(s, "testf.sha1").display()])?
  uu.fails(r)
  uu.stdout_is(r, "testf: FAILED open or read\n")
  uu.stderr_is(r, "sha1sum: testf: No such file or directory\nsha1sum: WARNING: 1 listed file could not be read\n")
}

# origin: uutils test_sha1sum::test_check_sha1
test test_uu_sha1sum_check_sha1 { |ctx|
  let s = checksum_scene(ctx)?
  uu.write(s, "testf", "foobar\n")?
  uu.write(s, "testf.sha1", "988881adc9fc3655077dc2d4d757d480b5ea0e11  testf\n")?
  let r = uu.invoke(s, "sha1sum", ["-c", uu.at(s, "testf.sha1").display()])?
  uu.succeeds(r)
  uu.stdout_only(r, "testf: OK\n")
}

# origin: uutils test_sha1sum::test_conflicting_arg
test test_uu_sha1sum_conflicting_arg { |ctx|
  let s = checksum_scene(ctx)?
  let check = uu.invoke(s, "sha1sum", ["--tag", "--check"])?
  uu.fails_with_code(check, 1)
  let text = uu.invoke(s, "sha1sum", ["--tag", "--text"])?
  uu.fails_with_code(text, 1)
}

# origin: uutils test_sha1sum::test_invalid_arg
test test_uu_sha1sum_invalid_arg { |ctx|
  let s = checksum_scene(ctx)?
  let r = uu.invoke(s, "sha1sum", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}
