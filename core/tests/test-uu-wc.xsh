##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_wc.rs.

use support.uu as uu

proc wc_scene(ctx: TestContext) [fs, error] -> Result[uu.Scene] {
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir with spaces")?
  for name in ["UTF_8_test.txt", "UTF_8_weirdchars.txt", "alice in wonderland.txt", "alice_in_wonderland.txt", "dir with spaces/.keep", "emptyfile.txt", "files0 with nonexistent.txt", "files0_list.txt", "files0_list_with_stdin.txt", "lorem_ipsum.txt", "manyemptylines.txt", "moby_dick.txt", "notrailingnewline.txt", "onelongemptyline.txt", "onelongword.txt"] {
    uu.fixture(s, "wc", name, name)?
  }
  Ok(s)
}

proc fixture_input(s: uu.Scene, name: Str) [fs, error] -> Result[Bytes] {
  uu.read(s, name)
}

# A producer must receive each result before supplying the next name.
proc read_exact(reader: Int, count: Int, deadline: Int) [process, fs, error, time] -> Result[Bytes] {
  var result = b""
  while result.len() < count {
    assert time.now() < deadline, "wc did not produce the next progressive result"
    if "readable" in unix.poll_fd(reader, ["readable"], timeout_ms: 1)? {
      result = bytes.concat([result, unix.read_fd(reader, count - result.len())?])
    }
  }
  Ok(result)
}

# origin: uutils test_wc::files0_from_dir
test test_uu_wc_files0_from_dir { |ctx|
  let s = wc_scene(ctx)?
  let r1 = uu.invoke(s, "wc", ["--files0-from=dir with spaces"])?
  uu.fails(r1)
  uu.stderr_only(r1, "wc: 'dir with spaces': read error: Is a directory\n")
  let r2 = uu.invoke(s, "wc", ["--files0-from=."])?
  uu.fails(r2)
  uu.stderr_only(r2, "wc: .: read error: Is a directory\n")
  let r3 = uu.invoke_from_path(s, "wc", ["--files0-from=-"], s.root)?
  uu.fails(r3)
  uu.stderr_only(r3, "wc: -: read error: Is a directory\n")
}

# origin: uutils test_wc::test_args_override
test test_uu_wc_args_override { |ctx|
  let s = wc_scene(ctx)?
  let r1 = uu.invoke(s, "wc", ["-ll", "-l", "alice_in_wonderland.txt"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "5 alice_in_wonderland.txt\n")
  let r2 = uu.invoke(s, "wc", ["--total=always", "--total=never", "alice_in_wonderland.txt"], stdin: b"")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "  5  57 302 alice_in_wonderland.txt\n")
}

# origin: uutils test_wc::test_ascii_control
test test_uu_wc_ascii_control { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-w"], stdin: b"\x01\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "1\n")
}

# origin: uutils test_wc::test_count_bytes_large_stdin
test test_uu_wc_count_bytes_large_stdin { |ctx|
  let s = wc_scene(ctx)?
  for size in [0, 1, 42, 16377, 16383, 16384, 16385, 16387, 32768, 65536, 81920, 98304, 114688, 131072] {
    let input = bytes.concat([b"a" for _ in range(size)])
    let r = uu.invoke(s, "wc", ["-c"], stdin: input)?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, bytes.from_text(f"{size}\n"))
  }
}

# origin: uutils test_wc::test_file_bytes_dictate_width
test test_uu_wc_file_bytes_dictate_width { |ctx|
  let s = wc_scene(ctx)?
  let r1 = uu.invoke(s, "wc", ["-lw", "onelongemptyline.txt"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "    1     0 onelongemptyline.txt\n")
  let r2 = uu.invoke(s, "wc", ["-lw", "emptyfile.txt"], stdin: b"")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "0 0 emptyfile.txt\n")
  let r3 = uu.invoke(s, "wc", ["-lwc", "alice_in_wonderland.txt", "lorem_ipsum.txt"], stdin: b"")?
  uu.succeeds(r3)
  uu.stdout_is(r3, "   5   57  302 alice_in_wonderland.txt\n  13  109  772 lorem_ipsum.txt\n  18  166 1074 total\n")
  let r4 = uu.invoke(s, "wc", ["-lwc", "emptyfile.txt", "."], stdin: b"")?
  uu.fails(r4)
  uu.stdout_is(r4, "      0       0       0 emptyfile.txt\n      0       0       0 .\n      0       0       0 total\n")
}

# origin: uutils test_wc::test_file_empty
test test_uu_wc_file_empty { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-clmwL", "emptyfile.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "0 0 0 0 0 emptyfile.txt\n")
}

# origin: uutils test_wc::test_file_many_empty_lines
test test_uu_wc_file_many_empty_lines { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-clmwL", "manyemptylines.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "100   0 100 100   0 manyemptylines.txt\n")
}

# origin: uutils test_wc::test_file_one_long_line_only_spaces
test test_uu_wc_file_one_long_line_only_spaces { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-clmwL", "onelongemptyline.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "    1     0 10001 10001 10000 onelongemptyline.txt\n")
}

# origin: uutils test_wc::test_file_one_long_word
test test_uu_wc_file_one_long_word { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-clmwL", "onelongword.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "    1     1 10001 10001 10000 onelongword.txt\n")
}

# origin: uutils test_wc::test_file_single_line_no_trailing_newline
test test_uu_wc_file_single_line_no_trailing_newline { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-clmwL", "notrailingnewline.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "1 1 2 2 1 notrailingnewline.txt\n")
}

# origin: uutils test_wc::test_files0_disabled_files_argument
test test_uu_wc_files0_disabled_files_argument { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["--files0-from=files0_list.txt", "lorem_ipsum.txt"])?
  uu.fails(r)
  uu.stderr_contains(r, "extra operand 'lorem_ipsum.txt'\nfile operands cannot be combined with --files0-from")
  uu.no_stdout(r)
}

# origin: uutils test_wc::test_files0_errors_quoting
test test_uu_wc_files0_errors_quoting { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["--files0-from=files0 with nonexistent.txt"])?
  uu.fails(r)
  uu.stderr_is(r, "wc: this_file_does_not_exist.txt: No such file or directory\nwc: 'files0 with nonexistent.txt':2: invalid zero-length file name\nwc: 'this file does not exist.txt': No such file or directory\nwc: \"this files doesn't exist either.txt\": No such file or directory\n")
  uu.stdout_is(r, "0 0 0 total\n")
}

# origin: uutils test_wc::test_files0_from
test test_uu_wc_files0_from { |ctx|
  let s = wc_scene(ctx)?
  let r1 = uu.invoke(s, "wc", ["--files0-from=files0_list.txt"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "  13  109  772 lorem_ipsum.txt\n  18  204 1115 moby_dick.txt\n   5   57  302 alice_in_wonderland.txt\n  36  370 2189 total\n")
  let r2 = uu.invoke(s, "wc", ["--files0-from=-"], stdin: fixture_input(s, "files0_list.txt")?)?
  uu.succeeds(r2)
  uu.stdout_is(r2, "13 109 772 lorem_ipsum.txt\n18 204 1115 moby_dick.txt\n5 57 302 alice_in_wonderland.txt\n36 370 2189 total\n")
}

# origin: uutils test_wc::test_files0_from_with_stdin
test test_uu_wc_files0_from_with_stdin { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["--files0-from=-"], stdin: b"lorem_ipsum.txt")?
  uu.succeeds(r)
  uu.stdout_is(r, "13 109 772 lorem_ipsum.txt\n")
}

# origin: uutils test_wc::test_files0_from_with_stdin_in_file
test test_uu_wc_files0_from_with_stdin_in_file { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["--files0-from=files0_list_with_stdin.txt"], stdin: fixture_input(s, "alice_in_wonderland.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     13     109     772 lorem_ipsum.txt\n     18     204    1115 moby_dick.txt\n      5      57     302 -\n     36     370    2189 total\n")
}

# origin: uutils test_wc::test_files0_from_with_stdin_try_read_from_stdin
test test_uu_wc_files0_from_with_stdin_try_read_from_stdin { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["--files0-from=-"], stdin: b"-")?
  uu.fails(r)
  uu.stderr_contains(r, "when reading file names from standard input, no file name of '-' allowed")
  uu.no_stdout(r)
}

# origin: uutils test_wc::test_files0_progressive_stream
test test_uu_wc_files0_progressive_stream { |ctx|
  let s = wc_scene(ctx)?
  uu.mkfifo(s, "stream-input")?
  uu.mkfifo(s, "stream-output")?
  uu.mkfifo(s, "stream-errors")?
  let reader = unix.open_fd(uu.at(s, "stream-output"), nonblock: true)?
  defer unix.close_fd(reader)
  let errors = unix.open_fd(uu.at(s, "stream-errors"), nonblock: true)?
  defer unix.close_fd(errors)
  let words = uu.argv(s, "wc", [p"--files0-from=-"])?
  let plan = process.command_argv(p"/bin/sh", [p"sh", p"-c", Path(r"""exec "$@" <stream-input >stream-output 2>stream-errors """), p"sh"].extend(words), s.root, {}, b"", uu.at(s, "outer-out"), uu.at(s, "outer-err"), timeout: 10s)
  let child = spawn plan?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  let deadline = time.now() + 5000
  var writer: Int? = null
  while writer == null {
    assert time.now() < deadline, "wc did not open stdin"
    match unix.open_fd(uu.at(s, "stream-input"), write: true, nonblock: true) {
      Ok(fd) => writer = fd,
      Err(failure) => { assert failure.errno == 6, failure.message; time.sleep(1ms)? },
    }
  }
  let producer = writer ?? -1
  var producer_open = true
  defer { if producer_open { unix.close_fd(producer) } }
  assert unix.write_fd(producer, b"moby_dick.txt\0")? == 14
  assert read_exact(reader, 26, deadline)? == b"18 204 1115 moby_dick.txt\n"
  assert unix.write_fd(producer, b"lorem_ipsum.txt\0")? == 16
  assert read_exact(reader, 27, deadline)? == b"13 109 772 lorem_ipsum.txt\n"
  assert unix.write_fd(producer, b"\0")? == 1
  assert read_exact(errors, 39, deadline)? == b"wc: -:3: invalid zero-length file name\n"
  assert unix.write_fd(producer, b"alice_in_wonderland.txt\0")? == 24
  assert read_exact(reader, 33, deadline)? == b"5 57 302 alice_in_wonderland.txt\n"
  unix.close_fd(producer)?
  producer_open = false
  let done = process.wait_timeout([child], 5s)?
  assert done != null, "wc should finish"
  assert ! done.status.exited_with(0)
  assert read_exact(reader, 18, time.now() + 1000)? == b"36 370 2189 total\n"
  assert unix.read_fd(reader, 1)? == b""
  assert unix.read_fd(errors, 1)? == b""
}

# origin: uutils test_wc::test_files0_stops_after_stdout_write_error
test test_uu_wc_files0_stops_after_stdout_write_error { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["--files0-from=-", "--total=never"], stdin: b"/dev/null\0/dev/null\0/dev/null\0", stdout: p"/dev/full")?
  uu.fails(r)
  assert r.stderr.utf8()?.split("write error: No space left on device").len() - 1 == 1, r.stderr.utf8()?
}

# origin: uutils test_wc::test_files_from_pseudo_filesystem
test test_uu_wc_files_from_pseudo_filesystem { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-c", "/proc/cpuinfo"])?
  uu.succeeds(r)
  assert r.stdout != b"0 /proc/cpuinfo\n"
  if p"/sys/kernel/profiling".exists()? {
    let profile = uu.invoke(s, "wc", ["-c", "/sys/kernel/profiling"])?
    uu.succeeds(profile)
    let actual = p"/sys/kernel/profiling".read_bytes()?.len()
    uu.stdout_is(profile, f"{actual} /sys/kernel/profiling\n")
  }
}

# origin: uutils test_wc::test_gnu_compatible_quotation
test test_uu_wc_gnu_compatible_quotation { |ctx|
  let s = wc_scene(ctx)?
  uu.mkdir(s, "some-dir1")?
  uu.touch(s, "some-dir1/12\n34.txt")?
  let r = uu.invoke(s, "wc", ["some-dir1/12\n34.txt"])?
  uu.succeeds(r)
  uu.stdout_is(r, "0 0 0 'some-dir1/12'$'\\n''34.txt'\n")
}

# origin: uutils test_wc::test_invalid_arg
test test_uu_wc_invalid_arg { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["--definitely-invalid"], stdin: b"")?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_wc::test_invalid_byte_sequence_word_count
test test_uu_wc_invalid_byte_sequence_word_count { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", [], stdin: b"a \xff b\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "      1       3       6\n")
}

# origin: uutils test_wc::test_multiple_default
test test_uu_wc_multiple_default { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["lorem_ipsum.txt", "moby_dick.txt", "alice_in_wonderland.txt", "alice in wonderland.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "  13  109  772 lorem_ipsum.txt\n  18  204 1115 moby_dick.txt\n   5   57  302 alice_in_wonderland.txt\n   5   57  302 alice in wonderland.txt\n  41  427 2491 total\n")
}

# origin: uutils test_wc::test_posixly_correct_whitespace
test test_uu_wc_posixly_correct_whitespace { |ctx|
  let s = wc_scene(ctx)?
  let input = bytes.from_text("word\u{00A0}word")
  let default = uu.invoke(s, "wc", ["-w"], stdin: input)?
  uu.succeeds(default)
  uu.stdout_is(default, "2\n")
  let posix = uu.invoke(s, "wc", ["-w"], stdin: input, vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(posix)
  uu.stdout_is(posix, "1\n")
}

# origin: uutils test_wc::test_read_error_order_with_stderr_to_stdout
test test_uu_wc_read_error_order_with_stderr_to_stdout { |ctx|
  let s = wc_scene(ctx)?
  uu.mkdir(s, "ioerrdir")?
  let words = uu.argv(s, "wc", [p"ioerrdir"])?
  let out = uu.at(s, "merged-output")
  let err = uu.at(s, "outer-errors")
  let plan = process.command_argv(p"/bin/sh", [p"sh", p"-c", Path(r"""exec "$@" 2>&1 """), p"sh"].extend(words), s.root, {}, b"", out, err)
  let status = process.run(plan)?
  assert ! status.exited_with(0)
  assert out.read_bytes()? == b"      0       0       0 ioerrdir\nwc: ioerrdir: Is a directory\n"
  assert err.read_bytes()? == b""
}

# origin: uutils test_wc::test_read_from_directory_error
test test_uu_wc_read_from_directory_error { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["."])?
  uu.fails(r)
  uu.stderr_contains(r, ".: Is a directory")
  uu.stdout_is(r, "      0       0       0 .\n")
}

# origin: uutils test_wc::test_read_from_nonexistent_file
test test_uu_wc_read_from_nonexistent_file { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["bogusfile"])?
  uu.fails(r)
  uu.stderr_only(r, "wc: bogusfile: No such file or directory\n")
}

# origin: uutils test_wc::test_simd_respects_glibc_tunables
test test_uu_wc_simd_respects_glibc_tunables { |ctx|
  let s = wc_scene(ctx)?
  let debug = uu.invoke(s, "wc", ["-l", "--debug", "/dev/null"], vars: {GLIBC_TUNABLES: "glibc.cpu.hwcaps=-AVX2,-AVX512F"})?
  uu.succeeds(debug)
  let diagnostic = debug.stderr.utf8()?
  assert ! ("using hardware support" in diagnostic), diagnostic
  assert "support not detected" in diagnostic, diagnostic
  for lines in [0, 1, 7, 128, 513, 999] {
    let content = bytes.from_text([f"{i}\n" for i in range(lines)].join(""))
    let base = uu.invoke(s, "wc", ["-l"], stdin: content)?
    uu.succeeds(base)
    let no_avx512 = uu.invoke(s, "wc", ["-l"], stdin: content, vars: {GLIBC_TUNABLES: "glibc.cpu.hwcaps=-AVX512F"})?
    uu.succeeds(no_avx512)
    let no_avx2_avx512 = uu.invoke(s, "wc", ["-l"], stdin: content, vars: {GLIBC_TUNABLES: "glibc.cpu.hwcaps=-AVX2,-AVX512F"})?
    uu.succeeds(no_avx2_avx512)
    assert base.stdout.utf8()?.trim() == no_avx512.stdout.utf8()?.trim()
    assert base.stdout.utf8()?.trim() == no_avx2_avx512.stdout.utf8()?.trim()
  }
}

# origin: uutils test_wc::test_single_all_counts
test test_uu_wc_single_all_counts { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-c", "-l", "-L", "-m", "-w", "alice_in_wonderland.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "  5  57 302 302  66 alice_in_wonderland.txt\n")
}

# origin: uutils test_wc::test_single_default
test test_uu_wc_single_default { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["moby_dick.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "  18  204 1115 moby_dick.txt\n")
}

# origin: uutils test_wc::test_single_only_bytes
test test_uu_wc_single_only_bytes { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-c", "lorem_ipsum.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "772 lorem_ipsum.txt\n")
}

# origin: uutils test_wc::test_single_only_lines
test test_uu_wc_single_only_lines { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-l", "moby_dick.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "18 moby_dick.txt\n")
}

# origin: uutils test_wc::test_stdin_all_counts
test test_uu_wc_stdin_all_counts { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-c", "-m", "-l", "-L", "-w"], stdin: fixture_input(s, "alice_in_wonderland.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "      5      57     302     302      66\n")
}

# origin: uutils test_wc::test_stdin_default
test test_uu_wc_stdin_default { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", [], stdin: fixture_input(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     13     109     772\n")
}

# origin: uutils test_wc::test_stdin_explicit
test test_uu_wc_stdin_explicit { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-"], stdin: fixture_input(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     13     109     772 -\n")
}

# origin: uutils test_wc::test_stdin_line_len_regression
test test_uu_wc_stdin_line_len_regression { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-L"], stdin: bytes.from_text("\n123456"))?
  uu.succeeds(r)
  uu.stdout_is(r, "6\n")
}

# origin: uutils test_wc::test_stdin_only_bytes
test test_uu_wc_stdin_only_bytes { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-c"], stdin: fixture_input(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "772\n")
}

# origin: uutils test_wc::test_total_always
test test_uu_wc_total_always { |ctx|
  let s = wc_scene(ctx)?
  let r1 = uu.invoke(s, "wc", ["lorem_ipsum.txt", "--total=always"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is(r1, " 13 109 772 lorem_ipsum.txt\n 13 109 772 total\n")
  let r2 = uu.invoke(s, "wc", ["lorem_ipsum.txt", "--total=al"], stdin: b"")?
  uu.succeeds(r2)
  uu.stdout_is(r2, " 13 109 772 lorem_ipsum.txt\n 13 109 772 total\n")
  let r3 = uu.invoke(s, "wc", ["lorem_ipsum.txt", "moby_dick.txt", "--total=always"], stdin: b"")?
  uu.succeeds(r3)
  uu.stdout_is(r3, "  13  109  772 lorem_ipsum.txt\n  18  204 1115 moby_dick.txt\n  31  313 1887 total\n")
}

# origin: uutils test_wc::test_total_auto
test test_uu_wc_total_auto { |ctx|
  let s = wc_scene(ctx)?
  let r1 = uu.invoke(s, "wc", ["lorem_ipsum.txt", "--total=auto"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is(r1, " 13 109 772 lorem_ipsum.txt\n")
  let r2 = uu.invoke(s, "wc", ["lorem_ipsum.txt", "--tot=au"], stdin: b"")?
  uu.succeeds(r2)
  uu.stdout_is(r2, " 13 109 772 lorem_ipsum.txt\n")
  let r3 = uu.invoke(s, "wc", ["lorem_ipsum.txt", "moby_dick.txt", "--total=auto"], stdin: b"")?
  uu.succeeds(r3)
  uu.stdout_is(r3, "  13  109  772 lorem_ipsum.txt\n  18  204 1115 moby_dick.txt\n  31  313 1887 total\n")
}

# origin: uutils test_wc::test_total_never
test test_uu_wc_total_never { |ctx|
  let s = wc_scene(ctx)?
  let r1 = uu.invoke(s, "wc", ["lorem_ipsum.txt", "--total=never"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is(r1, " 13 109 772 lorem_ipsum.txt\n")
  let r2 = uu.invoke(s, "wc", ["lorem_ipsum.txt", "moby_dick.txt", "--total=never"], stdin: b"")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "  13  109  772 lorem_ipsum.txt\n  18  204 1115 moby_dick.txt\n")
  let r3 = uu.invoke(s, "wc", ["lorem_ipsum.txt", "moby_dick.txt", "--total=n"], stdin: b"")?
  uu.succeeds(r3)
  uu.stdout_is(r3, "  13  109  772 lorem_ipsum.txt\n  18  204 1115 moby_dick.txt\n")
}

# origin: uutils test_wc::test_total_only
test test_uu_wc_total_only { |ctx|
  let s = wc_scene(ctx)?
  let r1 = uu.invoke(s, "wc", ["lorem_ipsum.txt", "--total=only"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "13 109 772\n")
  let r2 = uu.invoke(s, "wc", ["lorem_ipsum.txt", "moby_dick.txt", "--total=only"], stdin: b"")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "31 313 1887\n")
  let r3 = uu.invoke(s, "wc", ["lorem_ipsum.txt", "moby_dick.txt", "--t=o"], stdin: b"")?
  uu.succeeds(r3)
  uu.stdout_is(r3, "31 313 1887\n")
}

# origin: uutils test_wc::test_utf8
test test_uu_wc_utf8 { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-lwmcL"], stdin: fixture_input(s, "UTF_8_test.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "    303    2183   23025   23025      79\n")
}

# origin: uutils test_wc::test_utf8_all
test test_uu_wc_utf8_all { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-lwmcL"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     25      91     513     513      48\n")
}

# origin: uutils test_wc::test_utf8_bytes_chars
test test_uu_wc_utf8_bytes_chars { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-cm"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "    513     513\n")
}

# origin: uutils test_wc::test_utf8_bytes_chars_lines
test test_uu_wc_utf8_bytes_chars_lines { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-cml"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     25     513     513\n")
}

# origin: uutils test_wc::test_utf8_bytes_lines
test test_uu_wc_utf8_bytes_lines { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-cl"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     25     513\n")
}

# origin: uutils test_wc::test_utf8_chars
test test_uu_wc_utf8_chars { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-m"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "513\n")
}

# origin: uutils test_wc::test_utf8_chars_words
test test_uu_wc_utf8_chars_words { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-mw"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     91     513\n")
}

# origin: uutils test_wc::test_utf8_line_length_chars
test test_uu_wc_utf8_line_length_chars { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-Lm"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "    513      48\n")
}

# origin: uutils test_wc::test_utf8_line_length_chars_words
test test_uu_wc_utf8_line_length_chars_words { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-Lmw"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     91     513      48\n")
}

# origin: uutils test_wc::test_utf8_line_length_lines
test test_uu_wc_utf8_line_length_lines { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-Ll"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     25      48\n")
}

# origin: uutils test_wc::test_utf8_line_length_lines_chars
test test_uu_wc_utf8_line_length_lines_chars { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-Llm"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     25     513      48\n")
}

# origin: uutils test_wc::test_utf8_line_length_lines_words
test test_uu_wc_utf8_line_length_lines_words { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-Llw"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     25      91      48\n")
}

# origin: uutils test_wc::test_utf8_line_length_words
test test_uu_wc_utf8_line_length_words { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-Lw"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     91      48\n")
}

# origin: uutils test_wc::test_utf8_lines_chars
test test_uu_wc_utf8_lines_chars { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-ml"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     25     513\n")
}

# origin: uutils test_wc::test_utf8_lines_words_chars
test test_uu_wc_utf8_lines_words_chars { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-mlw"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "     25      91     513\n")
}

# origin: uutils test_wc::test_utf8_words
test test_uu_wc_utf8_words { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-w"], stdin: fixture_input(s, "UTF_8_weirdchars.txt")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "91\n")
}

# origin: uutils test_wc::test_zero_length_files
test test_uu_wc_zero_length_files { |ctx|
  let s = wc_scene(ctx)?
  let input = b"\0moby_dick.txt\0\0alice_in_wonderland.txt\0\0lorem_ipsum.txt\0"
  for length in [input.len(), input.len() - 1] {
    let r = uu.invoke(s, "wc", ["--files0-from=-"], stdin: input[0..length])?
    uu.fails(r)
    uu.stdout_is(r, "18 204 1115 moby_dick.txt\n5 57 302 alice_in_wonderland.txt\n13 109 772 lorem_ipsum.txt\n36 370 2189 total\n")
    uu.stderr_is(r, "wc: -:1: invalid zero-length file name\nwc: -:3: invalid zero-length file name\nwc: -:5: invalid zero-length file name\n")
  }
  let extra = uu.invoke(s, "wc", ["--files0-from=-"], stdin: bytes.concat([input, b"\0"]))?
  uu.fails(extra)
  uu.stdout_is(extra, "18 204 1115 moby_dick.txt\n5 57 302 alice_in_wonderland.txt\n13 109 772 lorem_ipsum.txt\n36 370 2189 total\n")
  uu.stderr_is(extra, "wc: -:1: invalid zero-length file name\nwc: -:3: invalid zero-length file name\nwc: -:5: invalid zero-length file name\nwc: -:7: invalid zero-length file name\n")
}

# origin: uutils test_wc::wc_w_words_with_emoji_separator
test test_uu_wc_wc_w_words_with_emoji_separator { |ctx|
  let s = wc_scene(ctx)?
  let r = uu.invoke(s, "wc", ["-w"], stdin: bytes.from_text("foo 💐 bar\n"))?
  uu.succeeds(r)
  uu.stdout_contains(r, "3")
}
