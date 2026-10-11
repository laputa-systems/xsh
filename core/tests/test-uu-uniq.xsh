##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_uniq.rs.

use support.uu as uu

# origin: uutils test_uniq::test_all_repeated_followed_by_filename
test test_uu_uniq_all_repeated_followed_by_filename { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test.txt", "a\na\n")?
  let r = uu.invoke(s, "uniq", ["--all-repeated", "test.txt"])?
  uu.succeeds(r)
  uu.stdout_is(r, "a\na\n")
}

# origin: uutils test_uniq::test_c_locale_counts_bytes
test test_uu_uniq_c_locale_counts_bytes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-w", "4"], stdin: bytes.from_text("가나다라마\n가나다바사\n"), vars: {LC_ALL: "C"})?
  uu.succeeds(r)
  uu.stdout_is(r, "가나다라마\n")
}

# origin: uutils test_uniq::test_case2
test test_uu_uniq_case2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", [], stdin: bytes.from_text("a\na\n"))?
  uu.succeeds(r)
  uu.stdout_is(r, "a\n")
}

# origin: uutils test_uniq::test_failed_write_is_reported
test test_uu_uniq_failed_write_is_reported { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-z"], stdin: b"hello", stdout: p"/dev/full")?
  uu.fails(r)
  uu.stderr_is(r, "uniq: write error: No space left on device\n")
}

# origin: uutils test_uniq::test_gnu_locale_fr_schar
test test_uu_uniq_gnu_locale_fr_schar { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "uniq", "locale-fr-schar.txt", "locale-fr-schar.txt")?
  let r = uu.invoke(s, "uniq", ["-f1", "locale-fr-schar.txt"], vars: {LC_ALL: "C"})?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/locale-fr-schar.txt".read_bytes()?
}

# origin: uutils test_uniq::test_group
test test_uu_uniq_group { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["--group"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/group.expected".read_bytes()?
}

# origin: uutils test_uniq::test_group_append
test test_uu_uniq_group_append { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "uniq", ["--group=append"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/group-append.expected".read_bytes()?
  }
  {
  let r = uu.invoke(s, "uniq", ["--group=a"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/group-append.expected".read_bytes()?
  }
}

# origin: uutils test_uniq::test_group_both
test test_uu_uniq_group_both { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "uniq", ["--group=both"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/group-both.expected".read_bytes()?
  }
  {
  let r = uu.invoke(s, "uniq", ["--group=bot"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/group-both.expected".read_bytes()?
  }
  {
  let r = uu.invoke(s, "uniq", ["--group=b"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/group-both.expected".read_bytes()?
  }
}

# origin: uutils test_uniq::test_group_followed_by_filename
test test_uu_uniq_group_followed_by_filename { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test.txt", "a\na\n")?
  let r = uu.invoke(s, "uniq", ["--group", "test.txt"])?
  uu.succeeds(r)
  uu.stdout_is(r, "a\na\n")
}

# origin: uutils test_uniq::test_group_prepend
test test_uu_uniq_group_prepend { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "uniq", ["--group=prepend"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/group-prepend.expected".read_bytes()?
  }
  {
  let r = uu.invoke(s, "uniq", ["--group=p"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/group-prepend.expected".read_bytes()?
  }
}

# origin: uutils test_uniq::test_group_separate
test test_uu_uniq_group_separate { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "uniq", ["--group=separate"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/group.expected".read_bytes()?
  }
  {
  let r = uu.invoke(s, "uniq", ["--group=s"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/group.expected".read_bytes()?
  }
}

# origin: uutils test_uniq::test_help_and_version_on_stdout
test test_uu_uniq_help_and_version_on_stdout { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "uniq", ["--help"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_contains(r, "Usage")
  }
  {
  let r = uu.invoke(s, "uniq", ["--version"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_contains(r, "uniq")
  }
}

# origin: uutils test_uniq::test_invalid_arg
test test_uu_uniq_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_uniq::test_nonexistent_input_file_error_matches_gnu
test test_uu_uniq_nonexistent_input_file_error_matches_gnu { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["nosuchfile.txt"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "uniq: nosuchfile.txt: No such file or directory\n")
}

# origin: uutils test_uniq::test_obsolete_skip_fields_not_read_after_double_dash
test test_uu_uniq_obsolete_skip_fields_not_read_after_double_dash { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["--", "-1"])?
  uu.fails(r)
  uu.stderr_contains(r, "-1")
}

# origin: uutils test_uniq::test_repeated_all_repeated_uses_final_delimiter
test test_uu_uniq_repeated_all_repeated_uses_final_delimiter { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "uniq", ["--all-repeated=prepend", "-D"], stdin: b"copper\ncopper\nsilver\ngold\ngold\nplatinum\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "copper\ncopper\ngold\ngold\n")
  }
  {
  let r = uu.invoke(s, "uniq", ["-D", "--all-repeated=separate"], stdin: b"copper\ncopper\nsilver\ngold\ngold\nplatinum\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "copper\ncopper\n\ngold\ngold\n")
  }
}

# origin: uutils test_uniq::test_repeated_skip_fields_takes_the_last
test test_uu_uniq_repeated_skip_fields_takes_the_last { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-f", "1", "-f", "2"], stdin: bytes.from_text("x y a\nz w a\n"))?
  uu.succeeds(r)
  uu.stdout_is(r, "x y a\n")
}

# origin: uutils test_uniq::test_single_default
test test_uu_uniq_single_default { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "uniq", "sorted.txt", "sorted.txt")?
  let r = uu.invoke(s, "uniq", ["sorted.txt"])?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-simple.expected".read_bytes()?
}

# origin: uutils test_uniq::test_single_default_output
test test_uu_uniq_single_default_output { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "uniq", "sorted.txt", "sorted.txt")?
  let expected = fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-simple.expected".read_bytes()?
  let r = uu.invoke(s, "uniq", ["sorted.txt", "sorted-output.txt"])?
  uu.succeeds(r)
  assert uu.read(s, "sorted-output.txt")? == expected
}

# origin: uutils test_uniq::test_stdin_all_repeated
test test_uu_uniq_stdin_all_repeated { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "uniq", ["--all-repeated"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-all-repeated.expected".read_bytes()?
  }
  {
  let r = uu.invoke(s, "uniq", ["--all-repeated=none"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-all-repeated.expected".read_bytes()?
  }
  {
  let r = uu.invoke(s, "uniq", ["--all-repeated=non"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-all-repeated.expected".read_bytes()?
  }
  {
  let r = uu.invoke(s, "uniq", ["--all-repeated=n"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-all-repeated.expected".read_bytes()?
  }
}

# origin: uutils test_uniq::test_stdin_all_repeated_prepend
test test_uu_uniq_stdin_all_repeated_prepend { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "uniq", ["--all-repeated=prepend"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-all-repeated-prepend.expected".read_bytes()?
  }
  {
  let r = uu.invoke(s, "uniq", ["--all-repeated=prepen"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-all-repeated-prepend.expected".read_bytes()?
  }
  {
  let r = uu.invoke(s, "uniq", ["--all-repeated=p"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-all-repeated-prepend.expected".read_bytes()?
  }
}

# origin: uutils test_uniq::test_stdin_all_repeated_separate
test test_uu_uniq_stdin_all_repeated_separate { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "uniq", ["--all-repeated=separate"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-all-repeated-separate.expected".read_bytes()?
  }
  {
  let r = uu.invoke(s, "uniq", ["--all-repeated=separat"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-all-repeated-separate.expected".read_bytes()?
  }
  {
  let r = uu.invoke(s, "uniq", ["--all-repeated=s"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-all-repeated-separate.expected".read_bytes()?
  }
}

# origin: uutils test_uniq::test_stdin_counts
test test_uu_uniq_stdin_counts { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-c"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-counts.expected".read_bytes()?
}

# origin: uutils test_uniq::test_stdin_default
test test_uu_uniq_stdin_default { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", [], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-simple.expected".read_bytes()?
}

# origin: uutils test_uniq::test_stdin_ignore_case
test test_uu_uniq_stdin_ignore_case { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-i"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-ignore-case.expected".read_bytes()?
}

# origin: uutils test_uniq::test_stdin_repeated_only
test test_uu_uniq_stdin_repeated_only { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-d"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-repeated-only.expected".read_bytes()?
}

# origin: uutils test_uniq::test_stdin_skip_1_char
test test_uu_uniq_stdin_skip_1_char { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-s1"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-chars.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-1-char.expected".read_bytes()?
}

# origin: uutils test_uniq::test_stdin_skip_21_fields
test test_uu_uniq_stdin_skip_21_fields { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-f21"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-fields.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-21-fields.expected".read_bytes()?
}

# origin: uutils test_uniq::test_stdin_skip_21_fields_obsolete
test test_uu_uniq_stdin_skip_21_fields_obsolete { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-21"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-fields.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-21-fields.expected".read_bytes()?
}

# origin: uutils test_uniq::test_stdin_skip_2_fields
test test_uu_uniq_stdin_skip_2_fields { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-f2"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-fields.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-2-fields.expected".read_bytes()?
}

# origin: uutils test_uniq::test_stdin_skip_2_fields_obsolete
test test_uu_uniq_stdin_skip_2_fields_obsolete { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-2"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-fields.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-2-fields.expected".read_bytes()?
}

# origin: uutils test_uniq::test_stdin_skip_5_chars
test test_uu_uniq_stdin_skip_5_chars { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-s5"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-chars.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-5-chars.expected".read_bytes()?
}

# origin: uutils test_uniq::test_stdin_skip_and_check_2_chars
test test_uu_uniq_stdin_skip_and_check_2_chars { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-s3", "-w2"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-chars.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/skip-3-check-2-chars.expected".read_bytes()?
}

# origin: uutils test_uniq::test_stdin_skip_invalid_fields_obsolete
test test_uu_uniq_stdin_skip_invalid_fields_obsolete { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-5q"])?
  uu.fails(r)
  uu.stderr_contains(r, "invalid option -- 'q'\n")
}

# origin: uutils test_uniq::test_stdin_unique_only
test test_uu_uniq_stdin_unique_only { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-u"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-unique-only.expected".read_bytes()?
}

# origin: uutils test_uniq::test_stdin_w1_multibyte
test test_uu_uniq_stdin_w1_multibyte { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-w1"], stdin: bytes.from_text("à\ná\n"), vars: {LC_ALL: "en_US.UTF-8"})?
  uu.succeeds(r)
  uu.stdout_is(r, "à\ná\n")
}

# origin: uutils test_uniq::test_stdin_zero_terminated
test test_uu_uniq_stdin_zero_terminated { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-z"], stdin: fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-zero-terminated.txt".read_bytes()?)?
  uu.succeeds(r)
  assert r.stdout == fp"{ctx.core_dir}/tests/data/uutils/uniq/sorted-zero-terminated.expected".read_bytes()?
}

