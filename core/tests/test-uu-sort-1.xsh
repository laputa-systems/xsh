##! Transcribed from the MIT-licensed uutils sort integration tests.
use support.uu

# Apply both descriptor limits before the applet launcher starts; the oracle uses the same argv.
proc limited(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran] {
  let out = uu.at(s, "limited.stdout")
  let err = uu.at(s, "limited.stderr")
  let argv = [p"/bin/sh", p"-c", Path(r"""ulimit -Sn 27 && ulimit -Hn 27 && exec "$@" """), p"sort-limit"].extend(uu.argv(s, "sort", [Path(arg) for arg in args])?)
  let status = process.run(process.command_argv(p"/bin/sh", argv, s.root, {}, b"", out, err))?
  Ok({util: "sort", args: args, status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# origin: uutils test_sort::diagnostics::test_plain_message_when_stderr_is_not_a_terminal
test test_uu_sort_diagnostics_plain_message_when_stderr_is_not_a_terminal { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-k2.3q", "/dev/null"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_only(r1, "sort: stray character in field spec: invalid field specification '2.3q'\n")
  let r2 = uu.invoke(s, "sort", ["-S", "8zz", "/dev/null"])?
  uu.fails_with_code(r2, 2)
  uu.stderr_only(r2, "sort: invalid suffix in -S argument '8zz'\n")
}

# origin: uutils test_sort::sort_empty_chunk
test test_uu_sort_sort_empty_chunk { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-S", "40b"], stdin: b"a\x0aa\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\na\n")
}

# origin: uutils test_sort::sort_multiple
test test_uu_sort_sort_multiple { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "no_trailing_newline1.txt", "no_trailing_newline1.txt")
  uu.fixture(s, "sort", "no_trailing_newline2.txt", "no_trailing_newline2.txt")
  let r1 = uu.invoke(s, "sort", ["no_trailing_newline1.txt", "no_trailing_newline2.txt"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\nb\nb\n")
}

# origin: uutils test_sort::test_args_check_conflict
test test_uu_sort_args_check_conflict { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-c", "-C"])?
  uu.fails(r1)
}

# origin: uutils test_sort::test_args_override
test test_uu_sort_args_override { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-f", "-f"], stdin: b"foo")?
  uu.succeeds(r1)
}

# origin: uutils test_sort::test_batch_size_invalid
test test_uu_sort_batch_size_invalid { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["--batch-size=0"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_contains(r1, "sort: invalid --batch-size argument '0'")
  uu.stderr_contains(r1, "sort: minimum --batch-size argument is '2'")
  let r2 = uu.invoke(s, "sort", ["-m", "--batch-size=a"])?
  uu.fails_with_code(r2, 2)
  uu.stderr_contains(r2, "sort: invalid --batch-size argument 'a'")
}

# origin: uutils test_sort::test_batch_size_too_large
test test_uu_sort_batch_size_too_large { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["--batch-size=18446744073709551616"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_contains(r1, "--batch-size argument '18446744073709551616' too large")
  let r2 = uu.invoke(s, "sort", ["--batch-size=18446744073709551616"])?
  uu.fails_with_code(r2, 2)
  uu.stderr_contains(r2, "maximum --batch-size argument with current rlimit is")
}

# origin: uutils test_sort::test_blanks
test test_uu_sort_blanks { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "blanks.txt", "blanks.txt")
  let r1 = uu.invoke(s, "sort", ["blanks.txt", "-b"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "blanks.expected", "blanks.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "blanks.expected")?)
  let r2 = uu.invoke(s, "sort", ["blanks.txt", "--debug", "-b"])?
  uu.succeeds(r2)
  uu.fixture(s, "sort", "blanks.expected.debug", "blanks.expected.debug")
  uu.stdout_is_bytes(r2, uu.read(s, "blanks.expected.debug")?)
  let r3 = uu.invoke(s, "sort", ["blanks.txt", "--ignore-leading-blanks"])?
  uu.succeeds(r3)
  uu.stdout_is_bytes(r3, uu.read(s, "blanks.expected")?)
  let r4 = uu.invoke(s, "sort", ["blanks.txt", "--debug", "--ignore-leading-blanks"])?
  uu.succeeds(r4)
  uu.stdout_is_bytes(r4, uu.read(s, "blanks.expected.debug")?)
}

# origin: uutils test_sort::test_check
test test_uu_sort_check { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "check_fail.txt", "check_fail.txt")
  let r1 = uu.invoke(s, "sort", ["-c", "check_fail.txt", "--buffer-size=10b"])?
  uu.fails(r1)
  uu.stderr_only(r1, "sort: check_fail.txt:6: disorder: 5\n")
  uu.fixture(s, "sort", "multiple_files.expected", "multiple_files.expected")
  let r2 = uu.invoke(s, "sort", ["-c", "multiple_files.expected"])?
  uu.succeeds(r2)
  uu.no_output(r2)
  let r3 = uu.invoke(s, "sort", ["--check", "check_fail.txt", "--buffer-size=10b"])?
  uu.fails(r3)
  uu.stderr_only(r3, "sort: check_fail.txt:6: disorder: 5\n")
  let r4 = uu.invoke(s, "sort", ["--check", "multiple_files.expected"])?
  uu.succeeds(r4)
  uu.no_output(r4)
  let r5 = uu.invoke(s, "sort", ["--check=diagnose-first", "check_fail.txt", "--buffer-size=10b"])?
  uu.fails(r5)
  uu.stderr_only(r5, "sort: check_fail.txt:6: disorder: 5\n")
  let r6 = uu.invoke(s, "sort", ["--check=diagnose-first", "multiple_files.expected"])?
  uu.succeeds(r6)
  uu.no_output(r6)
  let r7 = uu.invoke(s, "sort", ["--check=diagnose", "check_fail.txt", "--buffer-size=10b"])?
  uu.fails(r7)
  uu.stderr_only(r7, "sort: check_fail.txt:6: disorder: 5\n")
  let r8 = uu.invoke(s, "sort", ["--check=diagnose", "multiple_files.expected"])?
  uu.succeeds(r8)
  uu.no_output(r8)
  let r9 = uu.invoke(s, "sort", ["--check=d", "check_fail.txt", "--buffer-size=10b"])?
  uu.fails(r9)
  uu.stderr_only(r9, "sort: check_fail.txt:6: disorder: 5\n")
  let r10 = uu.invoke(s, "sort", ["--check=d", "multiple_files.expected"])?
  uu.succeeds(r10)
  uu.no_output(r10)
}

# origin: uutils test_sort::test_check_disorder_echoes_line_verbatim
test test_uu_sort_check_disorder_echoes_line_verbatim { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-n", "-c"], stdin: b"2\x0a1.10\x0a")?
  uu.fails(r1)
  uu.stderr_only(r1, "sort: -:2: disorder: 1.10\n")
  let r2 = uu.invoke(s, "sort", ["-n", "-c"], stdin: b"5\x0anan\x0a")?
  uu.fails(r2)
  uu.stderr_only(r2, "sort: -:2: disorder: nan\n")
  let r3 = uu.invoke(s, "sort", ["-g", "-c"], stdin: b"1e10\x0a1e5\x0a")?
  uu.fails(r3)
  uu.stderr_only(r3, "sort: -:2: disorder: 1e5\n")
}

# origin: uutils test_sort::test_check_silent
test test_uu_sort_check_silent { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "check_fail.txt", "check_fail.txt")
  let r1 = uu.invoke(s, "sort", ["-C", "check_fail.txt"])?
  uu.fails(r1)
  uu.no_output(r1)
  uu.fixture(s, "sort", "empty.txt", "empty.txt")
  let r2 = uu.invoke(s, "sort", ["-C", "empty.txt"])?
  uu.succeeds(r2)
  uu.no_output(r2)
  let r3 = uu.invoke(s, "sort", ["--check=silent", "check_fail.txt"])?
  uu.fails(r3)
  uu.no_output(r3)
  let r4 = uu.invoke(s, "sort", ["--check=silent", "empty.txt"])?
  uu.succeeds(r4)
  uu.no_output(r4)
  let r5 = uu.invoke(s, "sort", ["--check=quiet", "check_fail.txt"])?
  uu.fails(r5)
  uu.no_output(r5)
  let r6 = uu.invoke(s, "sort", ["--check=quiet", "empty.txt"])?
  uu.succeeds(r6)
  uu.no_output(r6)
  let r7 = uu.invoke(s, "sort", ["--check=silen", "check_fail.txt"])?
  uu.fails(r7)
  uu.no_output(r7)
  let r8 = uu.invoke(s, "sort", ["--check=silen", "empty.txt"])?
  uu.succeeds(r8)
  uu.no_output(r8)
  let r9 = uu.invoke(s, "sort", ["--check=quie", "check_fail.txt"])?
  uu.fails(r9)
  uu.no_output(r9)
  let r10 = uu.invoke(s, "sort", ["--check=quie", "empty.txt"])?
  uu.succeeds(r10)
  uu.no_output(r10)
  let r11 = uu.invoke(s, "sort", ["--check=s", "check_fail.txt"])?
  uu.fails(r11)
  uu.no_output(r11)
  let r12 = uu.invoke(s, "sort", ["--check=s", "empty.txt"])?
  uu.succeeds(r12)
  uu.no_output(r12)
  let r13 = uu.invoke(s, "sort", ["--check=q", "check_fail.txt"])?
  uu.fails(r13)
  uu.no_output(r13)
  let r14 = uu.invoke(s, "sort", ["--check=q", "empty.txt"])?
  uu.succeeds(r14)
  uu.no_output(r14)
}

# origin: uutils test_sort::test_check_unique
test test_uu_sort_check_unique { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-c", "-u"], stdin: b"A\x0aA\x0a")?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "sort: -:2: disorder: A\n")
}

# origin: uutils test_sort::test_check_unique_combined
test test_uu_sort_check_unique_combined { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-cu"], stdin: b"A\x0aA\x0a")?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "sort: -:2: disorder: A\n")
}

# origin: uutils test_sort::test_check_zero_terminated_success
test test_uu_sort_check_zero_terminated_success { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "zero-terminated.expected", "zero-terminated.expected")
  let r1 = uu.invoke(s, "sort", ["-z", "-c", "zero-terminated.expected"])?
  uu.succeeds(r1)
}

# origin: uutils test_sort::test_compress
test test_uu_sort_compress { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "ext_sort.txt", "ext_sort.txt")
  let r1 = uu.invoke(s, "sort", ["ext_sort.txt", "-n", "--compress-program", "gzip", "-S", "10"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "ext_sort.expected", "ext_sort.expected")
  uu.stdout_only_bytes(r1, uu.read(s, "ext_sort.expected")?)
}

# origin: uutils test_sort::test_compress_merge
test test_uu_sort_compress_merge { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "merge_ints_interleaved_1.txt", "merge_ints_interleaved_1.txt")
  uu.fixture(s, "sort", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_2.txt")
  uu.fixture(s, "sort", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_3.txt")
  let r1 = uu.invoke(s, "sort", ["--compress-program", "gzip", "-S", "10", "--batch-size=2", "-m", "--unique", "merge_ints_interleaved_1.txt", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_1.txt"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "merge_ints_interleaved.expected", "merge_ints_interleaved.expected")
  uu.stdout_only_bytes(r1, uu.read(s, "merge_ints_interleaved.expected")?)
}

# origin: uutils test_sort::test_default_unsorted_ints
test test_uu_sort_default_unsorted_ints { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "default_unsorted_ints.txt", "default_unsorted_ints.txt")
  let r1 = uu.invoke(s, "sort", ["default_unsorted_ints.txt"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "default_unsorted_ints.expected", "default_unsorted_ints.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "default_unsorted_ints.expected")?)
  let r2 = uu.invoke(s, "sort", ["default_unsorted_ints.txt", "--debug"])?
  uu.succeeds(r2)
  uu.fixture(s, "sort", "default_unsorted_ints.expected.debug", "default_unsorted_ints.expected.debug")
  uu.stdout_is_bytes(r2, uu.read(s, "default_unsorted_ints.expected.debug")?)
}

# origin: uutils test_sort::test_default_unsorted_ints2
test test_uu_sort_default_unsorted_ints2 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", [], stdin: b"9\x0a1909888\x0a000\x0a1\x0a2")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "000\n1\n1909888\n2\n9\n")
}

# origin: uutils test_sort::test_dictionary_order
test test_uu_sort_dictionary_order { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "dictionary_order.txt", "dictionary_order.txt")
  let r1 = uu.invoke(s, "sort", ["dictionary_order.txt", "-d"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "dictionary_order.expected", "dictionary_order.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "dictionary_order.expected")?)
  let r2 = uu.invoke(s, "sort", ["dictionary_order.txt", "--debug", "-d"])?
  uu.succeeds(r2)
  uu.fixture(s, "sort", "dictionary_order.expected.debug", "dictionary_order.expected.debug")
  uu.stdout_is_bytes(r2, uu.read(s, "dictionary_order.expected.debug")?)
}

# origin: uutils test_sort::test_dictionary_order2
test test_uu_sort_dictionary_order2 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-d"], stdin: b"a\xf0\x9f\x91\xa6\xf0\x9f\x8f\xbbaa\x09b\x0aaaaa\x09b")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a👦🏻aa\tb\naaaa\tb\n")
}

# origin: uutils test_sort::test_does_not_inherit_key_settings
test test_uu_sort_does_not_inherit_key_settings { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-k", "1b", "-n"], stdin: b" 1\x0a2\x0a   10\x0a")?
  uu.succeeds(r1)
  uu.stdout_only(r1, " 1\n   10\n2\n")
}

# origin: uutils test_sort::test_empty_input_empty_output
test test_uu_sort_empty_input_empty_output { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file", "test test test")
  let r1 = uu.invoke(s, "sort", [])?
  let r2 = uu.invoke(s, "sort", ["-o", "file"])?
  uu.succeeds(r2)
  assert uu.read_text(s, "file")? == ""
}

# origin: uutils test_sort::test_error_on_multiple_output_flags
test test_uu_sort_error_on_multiple_output_flags { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-o", "alpha", "-o", "beta"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_is(r1, "sort: multiple output files specified\n")
}

# origin: uutils test_sort::test_exponents_general
test test_uu_sort_exponents_general { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "exponents_general.txt", "exponents_general.txt")
  let r1 = uu.invoke(s, "sort", ["exponents_general.txt", "-g"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "exponents_general.expected", "exponents_general.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "exponents_general.expected")?)
  let r2 = uu.invoke(s, "sort", ["exponents_general.txt", "--debug", "-g"])?
  uu.succeeds(r2)
  uu.fixture(s, "sort", "exponents_general.expected.debug", "exponents_general.expected.debug")
  uu.stdout_is_bytes(r2, uu.read(s, "exponents_general.expected.debug")?)
}

# origin: uutils test_sort::test_exponents_positive_general
test test_uu_sort_exponents_positive_general { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-g"], stdin: b"1\x0a2e3\x0a1e-5")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1e-5\n1\n2e3\n")
}

# origin: uutils test_sort::test_exponents_positive_numeric
test test_uu_sort_exponents_positive_numeric { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "exponents-positive-numeric.txt", "exponents-positive-numeric.txt")
  let r1 = uu.invoke(s, "sort", ["exponents-positive-numeric.txt", "-n"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "exponents-positive-numeric.expected", "exponents-positive-numeric.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "exponents-positive-numeric.expected")?)
  let r2 = uu.invoke(s, "sort", ["exponents-positive-numeric.txt", "--debug", "-n"])?
  uu.succeeds(r2)
  uu.fixture(s, "sort", "exponents-positive-numeric.expected.debug", "exponents-positive-numeric.expected.debug")
  uu.stdout_is_bytes(r2, uu.read(s, "exponents-positive-numeric.expected.debug")?)
  let r3 = uu.invoke(s, "sort", ["exponents-positive-numeric.txt", "--numeric-sort"])?
  uu.succeeds(r3)
  uu.stdout_is_bytes(r3, uu.read(s, "exponents-positive-numeric.expected")?)
  let r4 = uu.invoke(s, "sort", ["exponents-positive-numeric.txt", "--debug", "--numeric-sort"])?
  uu.succeeds(r4)
  uu.stdout_is_bytes(r4, uu.read(s, "exponents-positive-numeric.expected.debug")?)
  let r5 = uu.invoke(s, "sort", ["exponents-positive-numeric.txt", "--sort=numeric"])?
  uu.succeeds(r5)
  uu.stdout_is_bytes(r5, uu.read(s, "exponents-positive-numeric.expected")?)
  let r6 = uu.invoke(s, "sort", ["exponents-positive-numeric.txt", "--debug", "--sort=numeric"])?
  uu.succeeds(r6)
  uu.stdout_is_bytes(r6, uu.read(s, "exponents-positive-numeric.expected.debug")?)
}

# origin: uutils test_sort::test_ext_sort_as64_bailout
test test_uu_sort_ext_sort_as64_bailout { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "multiple_decimals_general.txt", "multiple_decimals_general.txt")
  let r1 = uu.invoke(s, "sort", ["-g", "-S 5K", "multiple_decimals_general.txt"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "multiple_decimals_general.expected", "multiple_decimals_general.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "multiple_decimals_general.expected")?)
}

# origin: uutils test_sort::test_ext_sort_stable
test test_uu_sort_ext_sort_stable { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "ext_stable.txt", "ext_stable.txt")
  let r1 = uu.invoke(s, "sort", ["-n", "--stable", "-S", "0M", "ext_stable.txt"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "ext_stable.expected", "ext_stable.expected")
  uu.stdout_only_bytes(r1, uu.read(s, "ext_stable.expected")?)
}

# origin: uutils test_sort::test_ext_sort_zero_terminated
test test_uu_sort_ext_sort_zero_terminated { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "zero-terminated.txt", "zero-terminated.txt")
  let r1 = uu.invoke(s, "sort", ["-z", "-S", "10K", "zero-terminated.txt"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "zero-terminated.expected", "zero-terminated.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "zero-terminated.expected")?)
}

# origin: uutils test_sort::test_failed_write_is_reported
test test_uu_sort_failed_write_is_reported { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", [], stdin: b"hello", stdout: p"/dev/full")?
  uu.fails(r1)
  uu.stderr_is(r1, "sort: fflush failed: 'standard output': No space left on device\nsort: write error\n")
}

# origin: uutils test_sort::test_files0_from_dev_null_is_empty
test test_uu_sort_files0_from_dev_null_is_empty { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["--files0-from", "/dev/null"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_only(r1, "sort: no input from '/dev/null'\n")
}

# origin: uutils test_sort::test_files0_from_double_nul_is_invalid
test test_uu_sort_files0_from_double_nul_is_invalid { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["--files0-from", "-"], stdin: b"\x00\x00")?
  uu.fails_with_code(r1, 2)
  uu.stderr_only(r1, "sort: -:1: invalid zero-length file name\n")
}

# origin: uutils test_sort::test_files0_from_empty_input_file
test test_uu_sort_files0_from_empty_input_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")
  let r1 = uu.invoke(s, "sort", [])?
  let r2 = uu.invoke(s, "sort", ["--files0-from", "file"])?
  uu.fails_with_code(r2, 2)
  uu.stderr_only(r2, "sort: no input from 'file'\n")
}

# origin: uutils test_sort::test_files0_from_nonexistent_file_fails
test test_uu_sort_files0_from_nonexistent_file_fails { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["--files0-from", "missing_file"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_only(r1, "sort: open failed: missing_file: No such file or directory\n")
}

# origin: uutils test_sort::test_files0_from_reads_stdin_via_dash
test test_uu_sort_files0_from_reads_stdin_via_dash { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["--files0-from", "-"], stdin: b"-")?
  uu.fails_with_code(r1, 2)
  uu.stderr_only(r1, "sort: when reading file names from standard input, no file name of '-' allowed\n")
}

# origin: uutils test_sort::test_files0_from_rejects_extra_positional
test test_uu_sort_files0_from_rejects_extra_positional { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["--files0-from", "-", "foo"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_contains(r1, "sort: extra operand 'foo'\nfile operands cannot be combined with --files0-from\n")
  uu.no_stdout(r1)
}

# origin: uutils test_sort::test_files0_from_single_entry
test test_uu_sort_files0_from_single_entry { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "words")
  uu.append(s, "words", "mango\nkiwi")
  let r1 = uu.invoke(s, "sort", [])?
  let r2 = uu.invoke(s, "sort", ["--files0-from", "-"], stdin: b"words")?
  uu.succeeds(r2)
  uu.stdout_only(r2, "kiwi\nmango\n")
}

# origin: uutils test_sort::test_files0_from_single_entry_trailing_nul
test test_uu_sort_files0_from_single_entry_trailing_nul { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "words")
  uu.append(s, "words", "mango\nkiwi")
  let r1 = uu.invoke(s, "sort", [])?
  let r2 = uu.invoke(s, "sort", ["--files0-from", "-"], stdin: b"words\x00")?
  uu.succeeds(r2)
  uu.stdout_only(r2, "kiwi\nmango\n")
}

# origin: uutils test_sort::test_files0_from_single_nul_is_invalid
test test_uu_sort_files0_from_single_nul_is_invalid { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["--files0-from", "-"], stdin: b"\x00")?
  uu.fails_with_code(r1, 2)
  uu.stderr_only(r1, "sort: -:1: invalid zero-length file name\n")
}

# origin: uutils test_sort::test_files0_from_two_entries
test test_uu_sort_files0_from_two_entries { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "words")
  uu.append(s, "words", "mango\nkiwi")
  let r1 = uu.invoke(s, "sort", [])?
  let r2 = uu.invoke(s, "sort", ["--files0-from", "-"], stdin: b"words\x00words")?
  uu.succeeds(r2)
  uu.stdout_only(r2, "kiwi\nkiwi\nmango\nmango\n")
}

# origin: uutils test_sort::test_files0_from_two_entries_trailing_nul
test test_uu_sort_files0_from_two_entries_trailing_nul { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "words")
  uu.append(s, "words", "mango\nkiwi")
  let r1 = uu.invoke(s, "sort", [])?
  let r2 = uu.invoke(s, "sort", ["--files0-from", "-"], stdin: b"words\x00words\x00")?
  uu.succeeds(r2)
  uu.stdout_only(r2, "kiwi\nkiwi\nmango\nmango\n")
}

# origin: uutils test_sort::test_files0_from_unreadable_source
test test_uu_sort_files0_from_unreadable_source { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["--files0-from", "."])?
  uu.fails_with_code(r1, 2)
  uu.stderr_only(r1, "sort: cannot read file names from '.'\n")
}

# origin: uutils test_sort::test_files0_from_zero_length_entry_fails
test test_uu_sort_files0_from_zero_length_entry_fails { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["--files0-from", "-"], stdin: b"x\x00\x00y\x00\x00")?
  uu.fails_with_code(r1, 2)
  uu.stderr_only(r1, "sort: -:2: invalid zero-length file name\n")
}

# origin: uutils test_sort::test_g_float_hex
test test_uu_sort_g_float_hex { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-g"], stdin: b"0x123\x0a0x0\x0a0x2p10\x0a0x9p-10\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "0x0\n0x9p-10\n0x123\n0x2p10\n")
}

# origin: uutils test_sort::test_g_misc
test test_uu_sort_g_misc { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-g"], stdin: b"1\x0a100\x0a90\x0a'a\x0a85hello\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "'a\n1\n85hello\n90\n100\n")
}

# origin: uutils test_sort::test_human_block_sizes
test test_uu_sort_human_block_sizes { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "human_block_sizes.txt", "human_block_sizes.txt")
  let r1 = uu.invoke(s, "sort", ["human_block_sizes.txt", "-h"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "human_block_sizes.expected", "human_block_sizes.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "human_block_sizes.expected")?)
  let r2 = uu.invoke(s, "sort", ["human_block_sizes.txt", "--debug", "-h"])?
  uu.succeeds(r2)
  uu.fixture(s, "sort", "human_block_sizes.expected.debug", "human_block_sizes.expected.debug")
  uu.stdout_is_bytes(r2, uu.read(s, "human_block_sizes.expected.debug")?)
  let r3 = uu.invoke(s, "sort", ["human_block_sizes.txt", "--human-numeric-sort"])?
  uu.succeeds(r3)
  uu.stdout_is_bytes(r3, uu.read(s, "human_block_sizes.expected")?)
  let r4 = uu.invoke(s, "sort", ["human_block_sizes.txt", "--debug", "--human-numeric-sort"])?
  uu.succeeds(r4)
  uu.stdout_is_bytes(r4, uu.read(s, "human_block_sizes.expected.debug")?)
  let r5 = uu.invoke(s, "sort", ["human_block_sizes.txt", "--sort=human-numeric"])?
  uu.succeeds(r5)
  uu.stdout_is_bytes(r5, uu.read(s, "human_block_sizes.expected")?)
  let r6 = uu.invoke(s, "sort", ["human_block_sizes.txt", "--debug", "--sort=human-numeric"])?
  uu.succeeds(r6)
  uu.stdout_is_bytes(r6, uu.read(s, "human_block_sizes.expected.debug")?)
}

# origin: uutils test_sort::test_human_block_sizes2
test test_uu_sort_human_block_sizes2 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-h"], stdin: b"8981K\x0a909991M\x0a-8T\x0a21G\x0a0.8M")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-8T\n8981K\n0.8M\n909991M\n21G\n")
  let r2 = uu.invoke(s, "sort", ["--human-numeric-sort"], stdin: b"8981K\x0a909991M\x0a-8T\x0a21G\x0a0.8M")?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-8T\n8981K\n0.8M\n909991M\n21G\n")
  let r3 = uu.invoke(s, "sort", ["--sort=human-numeric"], stdin: b"8981K\x0a909991M\x0a-8T\x0a21G\x0a0.8M")?
  uu.succeeds(r3)
  uu.stdout_only(r3, "-8T\n8981K\n0.8M\n909991M\n21G\n")
}

# origin: uutils test_sort::test_human_blocks_r_and_q
test test_uu_sort_human_blocks_r_and_q { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-h"], stdin: b"1Q\x0a1R\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1R\n1Q\n")
}

# origin: uutils test_sort::test_human_numeric_whitespace
test test_uu_sort_human_numeric_whitespace { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "human-numeric-whitespace.txt", "human-numeric-whitespace.txt")
  let r1 = uu.invoke(s, "sort", ["human-numeric-whitespace.txt", "-h"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "human-numeric-whitespace.expected", "human-numeric-whitespace.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "human-numeric-whitespace.expected")?)
  let r2 = uu.invoke(s, "sort", ["human-numeric-whitespace.txt", "--debug", "-h"])?
  uu.succeeds(r2)
  uu.fixture(s, "sort", "human-numeric-whitespace.expected.debug", "human-numeric-whitespace.expected.debug")
  uu.stdout_is_bytes(r2, uu.read(s, "human-numeric-whitespace.expected.debug")?)
  let r3 = uu.invoke(s, "sort", ["human-numeric-whitespace.txt", "--human-numeric-sort"])?
  uu.succeeds(r3)
  uu.stdout_is_bytes(r3, uu.read(s, "human-numeric-whitespace.expected")?)
  let r4 = uu.invoke(s, "sort", ["human-numeric-whitespace.txt", "--debug", "--human-numeric-sort"])?
  uu.succeeds(r4)
  uu.stdout_is_bytes(r4, uu.read(s, "human-numeric-whitespace.expected.debug")?)
  let r5 = uu.invoke(s, "sort", ["human-numeric-whitespace.txt", "--sort=human-numeric"])?
  uu.succeeds(r5)
  uu.stdout_is_bytes(r5, uu.read(s, "human-numeric-whitespace.expected")?)
  let r6 = uu.invoke(s, "sort", ["human-numeric-whitespace.txt", "--debug", "--sort=human-numeric"])?
  uu.succeeds(r6)
  uu.stdout_is_bytes(r6, uu.read(s, "human-numeric-whitespace.expected.debug")?)
  let r7 = uu.invoke(s, "sort", ["human-numeric-whitespace.txt", "--sort=human-numeri"])?
  uu.succeeds(r7)
  uu.stdout_is_bytes(r7, uu.read(s, "human-numeric-whitespace.expected")?)
  let r8 = uu.invoke(s, "sort", ["human-numeric-whitespace.txt", "--debug", "--sort=human-numeri"])?
  uu.succeeds(r8)
  uu.stdout_is_bytes(r8, uu.read(s, "human-numeric-whitespace.expected.debug")?)
  let r9 = uu.invoke(s, "sort", ["human-numeric-whitespace.txt", "--sort=human"])?
  uu.succeeds(r9)
  uu.stdout_is_bytes(r9, uu.read(s, "human-numeric-whitespace.expected")?)
  let r10 = uu.invoke(s, "sort", ["human-numeric-whitespace.txt", "--debug", "--sort=human"])?
  uu.succeeds(r10)
  uu.stdout_is_bytes(r10, uu.read(s, "human-numeric-whitespace.expected.debug")?)
  let r11 = uu.invoke(s, "sort", ["human-numeric-whitespace.txt", "--sort=h"])?
  uu.succeeds(r11)
  uu.stdout_is_bytes(r11, uu.read(s, "human-numeric-whitespace.expected")?)
  let r12 = uu.invoke(s, "sort", ["human-numeric-whitespace.txt", "--debug", "--sort=h"])?
  uu.succeeds(r12)
  uu.stdout_is_bytes(r12, uu.read(s, "human-numeric-whitespace.expected.debug")?)
}

# origin: uutils test_sort::test_human_numeric_zero_stable
test test_uu_sort_human_numeric_zero_stable { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-hs"], stdin: b"0M\x0a0K\x0a-0K\x0a-P\x0a-0M\x0a")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0M\n0K\n-0K\n-P\n-0M\n")
}

# origin: uutils test_sort::test_ignore_case
test test_uu_sort_ignore_case { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "ignore_case.txt", "ignore_case.txt")
  let r1 = uu.invoke(s, "sort", ["ignore_case.txt", "-f"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "ignore_case.expected", "ignore_case.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "ignore_case.expected")?)
  let r2 = uu.invoke(s, "sort", ["ignore_case.txt", "--debug", "-f"])?
  uu.succeeds(r2)
  uu.fixture(s, "sort", "ignore_case.expected.debug", "ignore_case.expected.debug")
  uu.stdout_is_bytes(r2, uu.read(s, "ignore_case.expected.debug")?)
}

# origin: uutils test_sort::test_ignore_case_orders_punctuation_after_letters
test test_uu_sort_ignore_case_orders_punctuation_after_letters { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-f"], stdin: b"A\x0aa\x0a_\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "A\na\n_\n")
}

# origin: uutils test_sort::test_ignore_case_unique_orders_punctuation_after_letters
test test_uu_sort_ignore_case_unique_orders_punctuation_after_letters { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-fu"], stdin: b"a\x0a_\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\n_\n")
}

# origin: uutils test_sort::test_incompatible_options
test test_uu_sort_incompatible_options { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-hn"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_only(r1, "sort: options '-hn' are incompatible\n")
  let r2 = uu.invoke(s, "sort", ["-in"])?
  uu.fails_with_code(r2, 2)
  uu.stderr_only(r2, "sort: options '-in' are incompatible\n")
  let r3 = uu.invoke(s, "sort", ["-nR"])?
  uu.fails_with_code(r3, 2)
  uu.stderr_only(r3, "sort: options '-nR' are incompatible\n")
  let r4 = uu.invoke(s, "sort", ["-dfgiMnR"])?
  uu.fails_with_code(r4, 2)
  uu.stderr_only(r4, "sort: options '-dfgMnR' are incompatible\n")
  let r5 = uu.invoke(s, "sort", ["--sort=random", "-n"])?
  uu.fails_with_code(r5, 2)
  uu.stderr_only(r5, "sort: options '-nR' are incompatible\n")
  let r6 = uu.invoke(s, "sort", ["-c", "-o", "out"])?
  uu.fails_with_code(r6, 2)
  uu.stderr_only(r6, "sort: options '-co' are incompatible\n")
  let r7 = uu.invoke(s, "sort", ["-C", "-o", "out"])?
  uu.fails_with_code(r7, 2)
  uu.stderr_only(r7, "sort: options '-Co' are incompatible\n")
  let r8 = uu.invoke(s, "sort", ["-c", "-C"])?
  uu.fails_with_code(r8, 2)
  uu.stderr_only(r8, "sort: options '-cC' are incompatible\n")
}

# origin: uutils test_sort::test_inherits_key_settings
test test_uu_sort_inherits_key_settings { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-k", "1", "-n"], stdin: b" 1\x0a2\x0a   10\x0a")?
  uu.succeeds(r1)
  uu.stdout_only(r1, " 1\n2\n   10\n")
}

# origin: uutils test_sort::test_k_overflow
test test_uu_sort_k_overflow { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-k", "18446744073709551616"], stdin: b"2\x0a1\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1\n2\n")
}

# origin: uutils test_sort::test_key_takes_one_arg
test test_uu_sort_key_takes_one_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "keys_open_ended.txt", "keys_open_ended.txt")
  let r1 = uu.invoke(s, "sort", ["-k", "2.3", "keys_open_ended.txt"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "keys_open_ended.expected", "keys_open_ended.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "keys_open_ended.expected")?)
}

# origin: uutils test_sort::test_keys_blanks_with_char_idx
test test_uu_sort_keys_blanks_with_char_idx { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "keys_blanks.txt", "keys_blanks.txt")
  let r1 = uu.invoke(s, "sort", ["keys_blanks.txt", "-k", "1.2b"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "keys_blanks.expected", "keys_blanks.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "keys_blanks.expected")?)
  let r2 = uu.invoke(s, "sort", ["keys_blanks.txt", "--debug", "-k", "1.2b"])?
  uu.succeeds(r2)
  uu.fixture(s, "sort", "keys_blanks.expected.debug", "keys_blanks.expected.debug")
  uu.stdout_is_bytes(r2, uu.read(s, "keys_blanks.expected.debug")?)
}

# origin: uutils test_sort::test_keys_closed_range
test test_uu_sort_keys_closed_range { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "keys_closed_range.txt", "keys_closed_range.txt")
  let r1 = uu.invoke(s, "sort", ["keys_closed_range.txt", "-k", "2.2,2.2"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "keys_closed_range.expected", "keys_closed_range.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "keys_closed_range.expected")?)
  let r2 = uu.invoke(s, "sort", ["keys_closed_range.txt", "--debug", "-k", "2.2,2.2"])?
  uu.succeeds(r2)
  uu.fixture(s, "sort", "keys_closed_range.expected.debug", "keys_closed_range.expected.debug")
  uu.stdout_is_bytes(r2, uu.read(s, "keys_closed_range.expected.debug")?)
}

# origin: uutils test_sort::test_keys_custom_separator
test test_uu_sort_keys_custom_separator { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "keys_custom_separator.txt", "keys_custom_separator.txt")
  let r1 = uu.invoke(s, "sort", ["keys_custom_separator.txt", "-k", "2.2,2.2", "-t", "x"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "keys_custom_separator.expected", "keys_custom_separator.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "keys_custom_separator.expected")?)
  let r2 = uu.invoke(s, "sort", ["keys_custom_separator.txt", "--debug", "-k", "2.2,2.2", "-t", "x"])?
  uu.succeeds(r2)
  uu.fixture(s, "sort", "keys_custom_separator.expected.debug", "keys_custom_separator.expected.debug")
  uu.stdout_is_bytes(r2, uu.read(s, "keys_custom_separator.expected.debug")?)
}

# origin: uutils test_sort::test_keys_empty_match
test test_uu_sort_keys_empty_match { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-k", "1,1", "-t", "a"], stdin: b"a a a a\x0aaaaa\x0a")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a a a a\naaaa\n")
}

# origin: uutils test_sort::test_keys_ignore_flag
test test_uu_sort_keys_ignore_flag { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "keys_ignore_flag.txt", "keys_ignore_flag.txt")
  let r1 = uu.invoke(s, "sort", ["keys_ignore_flag.txt", "-k", "1n", "-b"])?
  uu.succeeds(r1)
  uu.fixture(s, "sort", "keys_ignore_flag.expected", "keys_ignore_flag.expected")
  uu.stdout_is_bytes(r1, uu.read(s, "keys_ignore_flag.expected")?)
  let r2 = uu.invoke(s, "sort", ["keys_ignore_flag.txt", "--debug", "-k", "1n", "-b"])?
  uu.succeeds(r2)
  uu.fixture(s, "sort", "keys_ignore_flag.expected.debug", "keys_ignore_flag.expected.debug")
  uu.stdout_is_bytes(r2, uu.read(s, "keys_ignore_flag.expected.debug")?)
}

# origin: uutils test_sort::test_keys_invalid_char_zero
test test_uu_sort_keys_invalid_char_zero { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-k", "1.0"])?
  uu.fails(r1)
  uu.stderr_only(r1, "sort: character offset is zero: invalid field specification '1.0'\n")
}

# origin: uutils test_sort::test_keys_invalid_field
test test_uu_sort_keys_invalid_field { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "sort", ["-k", "1."])?
  uu.fails(r1)
  uu.stderr_only(r1, "sort: invalid number after '.': invalid count at start of ''\n")
}

# origin: uutils test_sort::test_batch_size_above_fd_limit_is_rejected
test test_uu_sort_batch_size_above_fd_limit_is_rejected { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "gamma.txt", "delta\nalpha\n")
  let r = limited(s, ["--batch-size=31", "gamma.txt"])?
  uu.fails_with_code(r, 2)
  uu.stderr_contains(r, "--batch-size argument '31' too large")
  uu.stderr_contains(r, "maximum --batch-size argument with current rlimit is 24")
}

# origin: uutils test_sort::test_batch_size_at_fd_limit_is_accepted
test test_uu_sort_batch_size_at_fd_limit_is_accepted { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "gamma.txt", "delta\nalpha\n")
  let r = limited(s, ["--batch-size=24", "gamma.txt"])?
  uu.succeeds(r)
  uu.stdout_only(r, "alpha\ndelta\n")
}

# origin: uutils test_sort::test_buffer_size_limits_both_chunks
test test_uu_sort_buffer_size_limits_both_chunks { |ctx|
  let s = uu.scene(ctx)?
  let line = ["b" for _ in range(1022)].join("") + "\n"
  let input = bytes.from_text([line for _ in range(1200)].join(""))
  for args in [[], ["-k1,1"]] {
    let normal = uu.invoke(s, "sort", ["-u", "-T", "missing-directory"].extend(args), stdin: input)?
    uu.succeeds(normal)
    uu.stdout_only(normal, line)
    let bounded = uu.invoke(s, "sort", ["-u", "-T", "missing-directory", "-S", "768K"].extend(args), stdin: input)?
    uu.fails_with_code(bounded, 2)
    uu.no_stdout(bounded)
    uu.stderr_contains(bounded, "cannot create temporary file")
  }
}

# origin: uutils test_sort::test_buffer_sizes
test test_uu_sort_buffer_sizes { |ctx|
  # This corpus sorts 20,000 rows with nine buffer-size forms, including two external merges.
  test.timeout(ctx, 240s)
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "ext_sort.txt", "ext_sort.txt")
  uu.fixture(s, "sort", "ext_sort.expected", "ext_sort.expected")
  for size in ["0", "50K", "50k", "1M", "100M", "0%", "10%", "1000G", "10T"] {
    let r = uu.invoke(s, "sort", ["-n", "-S", size, "ext_sort.txt"])?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, uu.read(s, "ext_sort.expected")?)
  }
}

# origin: uutils test_sort::test_check_stops_early_without_panicking
test test_uu_sort_check_stops_early_without_panicking { |ctx|
  let s = uu.scene(ctx)?
  let half = ["c\n" for _ in range(20000)].join("")
  uu.write(s, "disorder.txt", f"{half}a\n{half}")
  let r = uu.invoke(s, "sort", ["-c", "--buffer-size=1K", "disorder.txt"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "sort: disorder.txt:20001: disorder: a\n")
}

# origin: uutils test_sort::test_compress_fail
test test_uu_sort_compress_fail { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "ext_sort.txt", "ext_sort.txt")
  let r = uu.invoke(s, "sort", ["ext_sort.txt", "-n", "--compress-program", "nonexistent-program", "-S", "10"])?
  uu.succeeds(r)
  uu.stderr_contains(r, "sort: could not run compress program 'nonexistent-program': No such file or directory")
  let baseline = uu.invoke(s, "sort", ["ext_sort.txt", "-n"])?
  uu.succeeds(baseline)
  assert r.stdout == baseline.stdout
}

# origin: uutils test_sort::test_debug_key_annotations
test test_uu_sort_debug_key_annotations { |ctx|
  let s = uu.scene(ctx)?
  var output = ""
  for mode in ["n", "h", "g"] {
    for key in [f"-k2{mode}", f"-k1.3{mode}", f"-k1{mode}"] {
      let r = uu.invoke(s, "sort", ["-s", key, "--debug"], stdin: b"1\n\n44\n33\n2\n")?
      uu.succeeds(r)
      output += r.stdout as Str
    }
    let r = uu.invoke(s, "sort", ["-s", "-k2g", "--debug"], stdin: b"1\t2\n2\t\n3\t1\n")?
    uu.succeeds(r)
    output += r.stdout as Str
  }
  for case in [
    {args: ["-s", "-k1M", "--debug"], input: "FEB\n\nJAN\n"},
    {args: ["-s", "-k2,2M", "--debug"], input: "FEB\n\nJAN\n"},
    {args: ["-s", "-k1M", "--debug"], input: "FEB\nJAZZ\n\nJAN\n"},
    {args: ["-s", "-k2,2M", "--debug"], input: "1\tFEB\n2\tJAZZ\n3\t\n4\tJAN\n"},
    {args: ["-s", "-k1M", "--debug"], input: "FEB\nJANZ\n\nJAN\n"},
    {args: ["-s", "-k2,2M", "--debug"], input: "1\tFEB\n2\tJANZ\n3\t\n4\tJAN\n"},
    {args: ["-s", "-g", "--debug"], input: " 1.2ignore\n 1.1e4ignore\n"},
    {args: ["-s", "-d", "--debug"], input: "\tb\n\t\ta\n"},
    {args: ["-s", "-k2,2", "--debug"], input: "a\n\n"},
    {args: ["-s", "-k1", "--debug"], input: "b\na\n"},
    {args: ["-s", "--debug", "-k1,1h"], input: "-0\n1\n-2\n--Mi-1\n-3\n-0\n"},
    {args: ["-b", "--debug"], input: " 1\n1\n"},
    {args: ["-s", "-b", "--debug"], input: " 1\n1\n"},
    {args: ["--debug"], input: " 1\n1\n"},
    {args: ["-s", "-k1n", "--debug"], input: "2,5\n2.4\n"},
    {args: ["-s", "-k1n", "--debug"], input: "2.,,3\n2.4\n"},
    {args: ["-s", "-k1n", "--debug"], input: "2,,3\n2.4\n"},
    {args: ["-s", "-n", "-z", "--debug"], input: "1a\x002b\x00"},
  ] {
    let r = uu.invoke(s, "sort", case.args, stdin: bytes.from_text(case.input))?
    uu.succeeds(r)
    output += r.stdout as Str
  }
  let zero = uu.invoke(s, "sort", ["-s", "-k2b,2", "--debug"], stdin: b"\x00\ta\n")?
  uu.succeeds(zero)
  output += zero.stdout.utf8()?.replace("\x00", with: "")
  for case in [
    {args: ["-s", "-k2.4b,2.3n", "--debug"], input: "A\tchr10\nB\tchr1\n"},
    {args: ["-s", "-k1.2b", "--debug"], input: "1 2\n1 3\n"},
  ] {
    let r = uu.invoke(s, "sort", case.args, stdin: bytes.from_text(case.input))?
    uu.succeeds(r)
    output += r.stdout as Str
  }
  uu.fixture(s, "sort", "debug-key-annotations.expected", "debug-key-annotations.expected")
  assert output == uu.read_text(s, "debug-key-annotations.expected")?
}

# Keep the three locale annotation launches and the original normalization together.
proc locale_annotations(s: uu.Scene, locale: Str) [fs, process, env, error] -> Result[Unit] {
  var output = ""
  for locale in ["C", locale] {
    let r = uu.invoke(s, "sort", ["--debug", "-k2g", "-k1b,1"], stdin: bytes.from_text("   1²---++3   1,234  Mi\n"), vars: {LC_ALL: locale})?
    uu.succeeds(r)
    output += r.stdout as Str
  }
  let r = uu.invoke(s, "sort", ["--debug", "-k1,1n", "-k1,1g", "-k1,1h", "-k2,2n", "-k2,2g", "-k2,2h", "-k3,3n", "-k3,3g", "-k3,3h"], stdin: b"+1234 1234Gi 1,234M\n", vars: {LC_ALL: locale})?
  uu.succeeds(r)
  output += r.stdout as Str
  let normalized = [if line.starts_with("^^ ") { "^ no match for key" } else { line } for line in output.lines()].join("\n") + "\n"
  uu.fixture(s, "sort", "debug-key-annotations-locale.expected", "debug-key-annotations-locale.expected")
  assert normalized == uu.read_text(s, "debug-key-annotations-locale.expected")?
  Ok()
}

# origin: uutils test_sort::test_debug_key_annotations_locale
test test_uu_sort_debug_key_annotations_locale { |ctx|
  let s = uu.scene(ctx)?
  if let Ok(locale) = env.get("LOCALE_FR_UTF8") {
    if locale != "none" {
      let probe = uu.invoke(s, "sort", ["-g", "--debug", "/dev/null"], vars: {LC_NUMERIC: locale, LC_MESSAGES: "C"})?
      if "numbers use .*,.* as a decimal point" in probe.stderr as Str { locale_annotations(s, locale) }
    }
  }
}

# The upstream literal-string guard is not a regular expression and suppresses this behavior.
test test_sort_locale_annotations_regression { |ctx|
  let s = uu.scene(ctx)?
  locale_annotations(s, "fr_FR.UTF-8")
}

# origin: uutils test_sort::test_dictionary_and_nonprinting_conflicts
test test_uu_sort_dictionary_and_nonprinting_conflicts { |ctx|
  let s = uu.scene(ctx)?
  for restricted in ["d", "i"] {
    for conflicting in ["n", "h", "g", "M"] {
      let global = uu.invoke(s, "sort", [f"-{restricted}{conflicting}"])?
      uu.fails(global)
    }
    for conflicting in ["n", "h", "g", "M"] {
      let local = uu.invoke(s, "sort", [f"-{restricted}", "-k", f"1,1{conflicting}"])?
      uu.succeeds(local)
    }
    for conflicting in ["n", "h", "g", "M"] {
      let mixed = uu.invoke(s, "sort", ["-k", f"1{restricted},1{conflicting}"])?
      uu.fails(mixed)
    }
  }
}

# origin: uutils test_sort::test_fifo_without_trailing_newline
test test_uu_sort_fifo_without_trailing_newline { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "FIFO")
  let out = uu.at(s, "fifo.stdout")
  let err = uu.at(s, "fifo.stderr")
  let plan = uu.command(s, "sort", ["FIFO"], stdout: out, stderr: err)?
  let child = spawn plan?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  uu.at(s, "FIFO").write("hello")
  let done = process.wait_timeout([child], 5s)?
  assert done != null, "sort did not exit after the FIFO writer closed"
  assert done.status.exit_code()? == 0
  assert out.read_bytes()? == b"hello\n"
}

# origin: uutils test_sort::test_files0_from_non_utf8_content
test test_uu_sort_files0_from_non_utf8_content { |ctx|
  let s = uu.scene(ctx)?
  let file = uu.at_bytes(s, b"a\xffb")?
  file.write(b"20\n10\n")
  uu.write_bytes(s, "list0", b"a\xffb\x00")
  let r = uu.invoke(s, "sort", ["--files0-from", "list0"])?
  uu.succeeds(r)
  uu.stdout_only(r, "10\n20\n")
}

# origin: uutils test_sort::test_files0_from_non_utf8_filename
test test_uu_sort_files0_from_non_utf8_filename { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["--files0-from", "-"], stdin: b"\xff")?
  uu.fails_with_code(r, 2)
  assert b"sort: cannot read" in r.stderr
}

# origin: uutils test_sort::test_g_arbitrary
test test_uu_sort_g_arbitrary { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-g"], stdin: bytes.from_text("3\n3.000000000000000000000000000000000000000000000000000000000000000004\n0.3000000000000000000000000000000000000000000000000000000000000000002e1\n0.03000000000000000000000000000000000000000000000000000000000000000003e2\n0.003000000000000000000000000000000000000000000000000000000000000000001e3\n10\n10.000000000000004\n1.0000000000000002e1\n0.10000000000000003e2\n0.010000000000000001e3"))?
  uu.succeeds(r)
  uu.stdout_is(r, "0.003000000000000000000000000000000000000000000000000000000000000000001e3\n0.03000000000000000000000000000000000000000000000000000000000000000003e2\n0.3000000000000000000000000000000000000000000000000000000000000000002e1\n3\n3.000000000000000000000000000000000000000000000000000000000000000004\n10\n0.010000000000000001e3\n1.0000000000000002e1\n0.10000000000000003e2\n10.000000000000004\n")
}

# origin: uutils test_sort::test_g_float_locale_decimal_separator
test test_uu_sort_g_float_locale_decimal_separator { |ctx|
  let s = uu.scene(ctx)?
  let comma = uu.invoke(s, "sort", ["-g", "--stable"], stdin: b"1,9\n1,10\n", vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(comma)
  uu.stdout_is(comma, "1,10\n1,9\n")
  let point = uu.invoke(s, "sort", ["-g", "--stable"], stdin: b"1.9\n1.10\n", vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(point)
  uu.stdout_is(point, "1.9\n1.10\n")
}

# origin: uutils test_sort::test_human_numeric_blank_thousands_sep_locale
test test_uu_sort_human_numeric_blank_thousands_sep_locale { |ctx|
  let s = uu.scene(ctx)?
  # The installed ISO Swedish locale reports its grouping blank as byte A0.
  let separator = bytes.from_ints([160])?
  let first = bytes.concat([b"1 1k 1 M 4", separator, b"003 1M"])
  let second = bytes.concat([b"2k 2M 2 k 4", separator, b"002 2"])
  let third = bytes.concat([b"3M 3 3 G 4", separator, b"001 3k"])
  uu.write_bytes(s, "blank-thousands.txt", bytes.concat([first, b"\n", second, b"\n", third, b"\n"]) )
  for case in [
    {key: "1", expected: bytes.concat([first, b"\n", second, b"\n", third, b"\n"])},
    {key: "2", expected: bytes.concat([third, b"\n", first, b"\n", second, b"\n"])},
    {key: "3", expected: bytes.concat([first, b"\n", second, b"\n", third, b"\n"])},
    {key: "5", expected: bytes.concat([third, b"\n", second, b"\n", first, b"\n"])},
  ] {
    let r = uu.invoke(s, "sort", ["-h", "-k", case.key, "blank-thousands.txt"], vars: {LC_ALL: "sv_SE"})?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, case.expected)
  }
}

# origin: uutils test_sort::test_invalid_buffer_size
test test_uu_sort_invalid_buffer_size { |ctx|
  let s = uu.scene(ctx)?
  for case in [
    {size: "asd", message: "sort: invalid -S argument 'asd'\n"},
    {size: "100f", message: "sort: invalid suffix in -S argument '100f'\n"},
    {size: "0x123%", message: "sort: invalid suffix in -S argument '0x123%'\n"},
    {size: "340282366920938463463374607431768211455%", message: "sort: invalid suffix in -S argument '340282366920938463463374607431768211455%'\n"},
  ] {
    let r = uu.invoke(s, "sort", ["-S", case.size])?
    uu.fails_with_code(r, 2)
    uu.stderr_only(r, case.message)
  }
  uu.fixture(s, "sort", "ext_sort.txt", "ext_sort.txt")
  let r = uu.invoke(s, "sort", ["-n", "-S", "1Y", "ext_sort.txt"])?
  uu.fails_with_code(r, 2)
  uu.stderr_only(r, "sort: -S argument '1Y' too large\n")
}
