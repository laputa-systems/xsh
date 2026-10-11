##! Native ports of the uutils md5sum integration tests.

use support.uu as uu

# origin: uutils test_md5sum::md5::test_single_file
test test_uu_md5sum_md5_single_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "md5sum", "input.txt", "input.txt")?
  uu.fixture(s, "md5sum", "md5.expected", "md5.expected")?
  let r = uu.invoke(s, "md5sum", ["input.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == uu.read_text(s, "md5.expected")?
}

# origin: uutils test_md5sum::md5::test_stdin
test test_uu_md5sum_md5_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "md5sum", "input.txt", "input.txt")?
  uu.fixture(s, "md5sum", "md5.expected", "md5.expected")?
  let r = uu.invoke(s, "md5sum", [], stdin: uu.read(s, "input.txt")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == uu.read_text(s, "md5.expected")?
}

# origin: uutils test_md5sum::md5::test_stdin_with_dash_directory
test test_uu_md5sum_md5_stdin_with_dash_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "md5sum", "input.txt", "input.txt")?
  uu.fixture(s, "md5sum", "md5.expected", "md5.expected")?
  uu.mkdir(s, "-")?
  let r = uu.invoke(s, "md5sum", [], stdin: uu.read(s, "input.txt")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == uu.read_text(s, "md5.expected")?
}

# origin: uutils test_md5sum::md5::test_zero
test test_uu_md5sum_md5_zero { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "md5sum", "input.txt", "input.txt")?
  uu.fixture(s, "md5sum", "md5.expected", "md5.expected")?
  let r = uu.invoke(s, "md5sum", ["--zero", "input.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split(" ")[0] == uu.read_text(s, "md5.expected")?
}

# origin: uutils test_md5sum::md5::test_check
test test_uu_md5sum_md5_check { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "md5sum", "input.txt", "input.txt")?
  uu.fixture(s, "md5sum", "md5.checkfile", "md5.checkfile")?
  let r = uu.invoke(s, "md5sum", ["--check", "md5.checkfile"], stdin: b"")?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "input.txt: OK\n")
}

# origin: uutils test_md5sum::md5::test_missing_file
test test_uu_md5sum_md5_missing_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "file1\n")?
  uu.write(s, "c", "file3\n")?
  let r = uu.invoke(s, "md5sum", ["a", "b", "c"], stdin: b"")?
  uu.fails(r)
  uu.stdout_contains(r, "a\n")
  uu.stdout_contains(r, "c\n")
  uu.stderr_contains(r, "b: No such file or directory")
}

# origin: uutils test_md5sum::test_invalid_arg
test test_uu_md5sum_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "md5sum", ["--definitely-invalid"], stdin: b"")?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_md5sum::test_conflicting_arg
test test_uu_md5sum_conflicting_arg { |ctx|
  let s = uu.scene(ctx)?
  let checking = uu.invoke(s, "md5sum", ["--tag", "--check"], stdin: b"")?
  uu.fails_with_code(checking, 1)
  let textmode = uu.invoke(s, "md5sum", ["--tag", "--text"], stdin: b"")?
  uu.fails_with_code(textmode, 1)
}

# origin: uutils test_md5sum::test_check_generate_round_trip_crlf_file
test test_uu_md5sum_check_generate_round_trip_crlf_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "f", b"abc\r\nd\r")?
  let generated = uu.invoke(s, "md5sum", ["f"], stdin: b"")?
  uu.succeeds(generated)
  uu.write_bytes(s, "CHECKSUM", generated.stdout)?
  let r = uu.invoke(s, "md5sum", ["--check", "CHECKSUM"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "f: OK\n")
}

# origin: uutils test_md5sum::test_with_escape_filename
test test_uu_md5sum_with_escape_filename { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a\nb")?
  let r = uu.invoke(s, "md5sum", ["--text", "a\nb"], stdin: b"")?
  uu.succeeds(r)
  assert r.stdout.utf8()?.starts_with("\\")
  assert r.stdout.utf8()?.trim().ends_with("a\\nb")
}

# origin: uutils test_md5sum::test_with_escape_filename_zero_text
test test_uu_md5sum_with_escape_filename_zero_text { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a\nb")?
  let r = uu.invoke(s, "md5sum", ["--text", "--zero", "a\nb"], stdin: b"")?
  uu.succeeds(r)
  assert ! r.stdout.utf8()?.starts_with("\\")
  uu.stdout_contains(r, "a\nb")
}

# origin: uutils test_md5sum::test_check_with_escape_filename
test test_uu_md5sum_check_with_escape_filename { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a\nb")?
  let generated = uu.invoke(s, "md5sum", ["--tag", "a\nb"], stdin: b"")?
  uu.succeeds(generated)
  assert generated.stdout.utf8()?.starts_with("\\MD5")
  uu.stdout_contains(generated, "a\\nb")
  uu.write_bytes(s, "check.md5", generated.stdout)?
  let r = uu.invoke(s, "md5sum", ["--strict", "-c", "check.md5"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "'a'$'\\n''b': OK\n")
}

# origin: uutils test_md5sum::test_check_md5sum
test test_uu_md5sum_check_md5sum { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", " b", "*c", "dd", " "] { uu.write(s, name, f"{name}\n")? }
  uu.write(s, "check.md5sum", "60b725f10c9c85c70d97880dfe8191b3  a\nbf35d7536c785cf06730d5a40301eba2   b\nf5b61709718c1ecf8db1aea8547d4698  *c\nb064a020db8018f18ff5ae367d01b212  dd\nd784fa8b6d98d27699781bd9a7cf19f0   ")?
  let r = uu.invoke(s, "md5sum", ["--strict", "-c", "check.md5sum"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "a: OK\n' b': OK\n'*c': OK\ndd: OK\n' ': OK\n")
}

# origin: uutils test_md5sum::test_check_md5sum_reverse_bsd
test test_uu_md5sum_check_md5sum_reverse_bsd { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", " b", "*c", "dd", " "] { uu.write(s, name, f"{name}\n")? }
  uu.write(s, "check.md5sum", "60b725f10c9c85c70d97880dfe8191b3  a\nbf35d7536c785cf06730d5a40301eba2   b\nf5b61709718c1ecf8db1aea8547d4698  *c\nb064a020db8018f18ff5ae367d01b212  dd\nd784fa8b6d98d27699781bd9a7cf19f0   ")?
  let r = uu.invoke(s, "md5sum", ["--strict", "-c", "check.md5sum"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "a: OK\n' b': OK\n'*c': OK\ndd: OK\n' ': OK\n")
}

# origin: uutils test_md5sum::test_check_md5sum_only_one_space
test test_uu_md5sum_check_md5sum_only_one_space { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", " b", "c"] { uu.write(s, name, f"{name}\n")? }
  uu.write(s, "check.md5sum", "60b725f10c9c85c70d97880dfe8191b3 a\nbf35d7536c785cf06730d5a40301eba2  b\n2cd6ee2c70b0bde53fbe6cac3c8b8bb1 c\n")?
  let r = uu.invoke(s, "md5sum", ["--strict", "-c", "check.md5sum"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "a: OK\n' b': OK\nc: OK\n")
}

# origin: uutils test_md5sum::test_check_md5sum_mixed_format
test test_uu_md5sum_check_md5sum_mixed_format { |ctx|
  let s = uu.scene(ctx)?
  for name in [" b", "*c", "dd", " "] { uu.write(s, name, f"{name}\n")? }
  uu.write(s, "check.md5sum", "bf35d7536c785cf06730d5a40301eba2  b\nf5b61709718c1ecf8db1aea8547d4698 *c\nb064a020db8018f18ff5ae367d01b212 dd\nd784fa8b6d98d27699781bd9a7cf19f0  ")?
  let r = uu.invoke(s, "md5sum", ["--strict", "-c", "check.md5sum"], stdin: b"")?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_md5sum::test_check_status_reports_malformed_input
test test_uu_md5sum_check_status_reports_malformed_input { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "md5sum", ["-c", "--status"], stdin: bytes.from_text("I'mNotAHash\n"))?
  uu.fails(r0)
  uu.stderr_contains(r0, "no properly formatted checksum lines found")
  uu.no_stdout(r0)
}

# origin: uutils test_md5sum::test_check_directory_error
test test_uu_md5sum_check_directory_error { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.write(s, "in.md5", "d41d8cd98f00b204e9800998ecf8427f  d\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r0)
  uu.stderr_contains(r0, "md5sum: d: Is a directory\n")
}

# origin: uutils test_md5sum::test_check_quiet
test test_uu_md5sum_check_quiet { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "in.md5", "d41d8cd98f00b204e9800998ecf8427e  f\n")?
  let r0 = uu.invoke(s, "md5sum", ["--quiet", "--check", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.succeeds(r0)
  uu.no_output(r0)
  uu.write(s, "in.md5", "d41d8cd98f00b204e9800998ecf8427f  f\n")?
  let r1 = uu.invoke(s, "md5sum", ["--quiet", "--check", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r1)
  uu.stdout_contains(r1, "f: FAILED")
  uu.stderr_contains(r1, "WARNING: 1 computed checksum did NOT match")
  let r2 = uu.invoke(s, "md5sum", ["--quiet", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r2)
  uu.stderr_contains(r2, "md5sum: the --quiet option is meaningful only when verifying checksums")
  let r3 = uu.invoke(s, "md5sum", ["--strict", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r3)
  uu.stderr_contains(r3, "md5sum: the --strict option is meaningful only when verifying checksums")
}

# origin: uutils test_md5sum::test_sha1_with_md5sum_should_fail
test test_uu_md5sum_sha1_with_md5sum_should_fail { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "f.sha1", "SHA1 (f) = d41d8cd98f00b204e9800998ecf8427e\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", uu.at(s, "f.sha1").display()], stdin: b"")?
  uu.fails(r0)
  uu.stderr_contains(r0, "f.sha1: no properly formatted checksum lines found")
  assert ! ("WARNING: 1 line is improperly formatted" in r0.stderr.utf8()?)
}

# origin: uutils test_md5sum::test_check_md5_comment_line
test test_uu_md5sum_check_md5_comment_line { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "foo-content\n")?
  uu.write(s, "MD5SUM", "# This is a comment\n8411029f3f5b781026a93db636aca721  foo\n# next comment is empty\n#")?
  let r0 = uu.invoke(s, "md5sum", ["--check", "MD5SUM"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "foo: OK")
  uu.no_stderr(r0)
}

# origin: uutils test_md5sum::test_check_strict_error
test test_uu_md5sum_check_strict_error { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "in.md5", "ERR\nERR\nd41d8cd98f00b204e9800998ecf8427e  f\nERR\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", "--strict", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r0)
  uu.stderr_contains(r0, "WARNING: 3 lines are improperly formatted")
}

# origin: uutils test_md5sum::test_continue_after_directory_error
test test_uu_md5sum_continue_after_directory_error { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.touch(s, "file")?
  uu.touch(s, "no_read_perms")?
  uu.set_mode(s, "no_read_perms", 200)?
  let r0 = uu.invoke(s, "md5sum", ["d", "dne", "no_read_perms", "file"], stdin: b"")?
  uu.fails(r0)
  uu.stdout_is(r0, "d41d8cd98f00b204e9800998ecf8427e  file\n")
  uu.stderr_is(r0, "md5sum: d: Is a directory\nmd5sum: dne: No such file or directory\nmd5sum: no_read_perms: Permission denied\n")
}

# origin: uutils test_md5sum::test_check_empty_line
test test_uu_md5sum_check_empty_line { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "in.md5", "d41d8cd98f00b204e9800998ecf8427e  f\n\nd41d8cd98f00b204e9800998ecf8427e  f\ninvalid\n\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.succeeds(r0)
  uu.stderr_contains(r0, "WARNING: 1 line is improperly formatted")
}

# origin: uutils test_md5sum::test_check_no_backslash_no_space
test test_uu_md5sum_check_no_backslash_no_space { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "in.md5", "MD5(f)= d41d8cd98f00b204e9800998ecf8427e\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is(r0, "f: OK\n")
}

# origin: uutils test_md5sum::test_check_md5_ignore_missing
test test_uu_md5sum_check_md5_ignore_missing { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "testf", "foobar\n")?
  uu.write(s, "testf.sha1", "14758f1afd44c09b7992073ccf00b43d  testf\n14758f1afd44c09b7992073ccf00b43d  testf2\n")?
  let r0 = uu.invoke(s, "md5sum", ["-c", uu.at(s, "testf.sha1").display()], stdin: b"")?
  uu.fails(r0)
  uu.stdout_contains(r0, "testf2: FAILED open or read")
  let r1 = uu.invoke(s, "md5sum", ["-c", "--ignore-missing", uu.at(s, "testf.sha1").display()], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "testf: OK\n")
  let r2 = uu.invoke(s, "md5sum", ["--ignore-missing", uu.at(s, "testf.sha1").display()], stdin: b"")?
  uu.fails(r2)
  uu.stderr_contains(r2, "md5sum: the --ignore-missing option is meaningful only when verifying checksums")
}

# origin: uutils test_md5sum::test_check_warn
test test_uu_md5sum_check_warn { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "in.md5", "d41d8cd98f00b204e9800998ecf8427e  f\nd41d8cd98f00b204e9800998ecf8427e  f\ninvalid\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", "--warn", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.succeeds(r0)
  uu.stderr_contains(r0, "in.md5: 3: improperly formatted MD5 checksum line")
  uu.stderr_contains(r0, "WARNING: 1 line is improperly formatted")
  let r1 = uu.invoke(s, "md5sum", ["--check", "--strict", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r1)
}

# origin: uutils test_md5sum::test_check_one_two_space_star
test test_uu_md5sum_check_one_two_space_star { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "empty")?
  uu.write(s, "in.md5", "d41d8cd98f00b204e9800998ecf8427e *empty\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is(r0, "empty: OK\n")
  uu.write(s, "in.md5", "d41d8cd98f00b204e9800998ecf8427e  *empty\n")?
  let r1 = uu.invoke(s, "md5sum", ["--check", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r1)
  uu.stdout_is(r1, "'*empty': FAILED open or read\n")
  uu.touch(s, "*empty")?
  let r2 = uu.invoke(s, "md5sum", ["--check", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "'*empty': OK\n")
}

# origin: uutils test_md5sum::test_start_error
test test_uu_md5sum_start_error { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "in.md5", "ERR\nd41d8cd98f00b204e9800998ecf8427e  f\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", "--strict", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r0)
  uu.stdout_is(r0, "f: OK\n")
  uu.stderr_contains(r0, "WARNING: 1 line is improperly formatted")
}

# origin: uutils test_md5sum::test_check_check_ignore_no_file
test test_uu_md5sum_check_check_ignore_no_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "in.md5", "d41d8cd98f00b204e9800998ecf8427f  missing\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", "--ignore-missing", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r0)
  uu.stderr_contains(r0, "in.md5: no file was verified")
}

# origin: uutils test_md5sum::test_star_to_start
test test_uu_md5sum_star_to_start { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "in.md5", "d41d8cd98f00b204e9800998ecf8427e *f\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_only(r0, "f: OK\n")
}

# origin: uutils test_md5sum::test_check_status
test test_uu_md5sum_check_status { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "in.md5", "MD5(f)= d41d8cd98f00b204e9800998ecf8427f\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", "--status", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r0)
  uu.no_output(r0)
}

# origin: uutils test_md5sum::test_check_space_star_or_not
test test_uu_md5sum_check_space_star_or_not { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "*c")?
  uu.write(s, "in.md5", "d41d8cd98f00b204e9800998ecf8427e *c\n\n        d41d8cd98f00b204e9800998ecf8427e a\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r0)
  uu.stdout_contains(r0, "c: FAILED")
  assert ! ("a: FAILED" in r0.stdout.utf8()?)
  uu.stderr_contains(r0, "WARNING: 1 line is improperly formatted")
  uu.write(s, "in.md5", "d41d8cd98f00b204e9800998ecf8427e a\n\n            d41d8cd98f00b204e9800998ecf8427e *c\n")?
  let r1 = uu.invoke(s, "md5sum", ["--check", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "a: OK")
  uu.no_stderr(r1)
}

# origin: uutils test_md5sum::test_check_md5_comment_only
test test_uu_md5sum_check_md5_comment_only { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "foo-content\n")?
  uu.write(s, "MD5SUM", "# This is a comment\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", "MD5SUM"], stdin: b"")?
  uu.fails(r0)
  uu.stderr_contains(r0, "no properly formatted checksum lines found")
}

# origin: uutils test_md5sum::test_check_md5_comment_leading_space
test test_uu_md5sum_check_md5_comment_leading_space { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "foo-content\n")?
  uu.write(s, "MD5SUM", " # This is a comment\n8411029f3f5b781026a93db636aca721  foo\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", "MD5SUM"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "foo: OK")
  uu.stderr_contains(r0, "WARNING: 1 line is improperly formatted")
}

# origin: uutils test_md5sum::test_check_status_code
test test_uu_md5sum_check_status_code { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "in.md5", "d41d8cd98f00b204e9800998ecf8427f  f\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", "--status", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r0)
  uu.no_output(r0)
}

# origin: uutils test_md5sum::test_incomplete_format
test test_uu_md5sum_incomplete_format { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "in.md5", "MD5 (\n")?
  let r0 = uu.invoke(s, "md5sum", ["--check", uu.at(s, "in.md5").display()], stdin: b"")?
  uu.fails(r0)
  uu.stderr_contains(r0, "no properly formatted checksum lines found")
}

