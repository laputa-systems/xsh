##! Transcribed from the uutils sort integration tests.

use support.uu as uu

proc fixture_case(ctx: TestContext, name: Str, options: List[Str]) [fs, process, env, error] {
  let input = f"{name}.txt"
  let expected = fp"{ctx.core_dir}/tests/data/uutils/sort/{name}.expected".read_bytes()?
  let debug_expected = fp"{ctx.core_dir}/tests/data/uutils/sort/{name}.expected.debug".read_bytes()?
  let plain_scene = uu.scene(ctx)?
  uu.fixture(plain_scene, "sort", input, input)?
  let plain = uu.invoke(plain_scene, "sort", [input].extend(options), timeout: 5s)?
  uu.succeeds(plain)
  uu.stdout_is_bytes(plain, expected)
  let debug_scene = uu.scene(ctx)?
  uu.fixture(debug_scene, "sort", input, input)?
  let debug = uu.invoke(debug_scene, "sort", [input, "--debug"].extend(options), timeout: 5s)?
  uu.succeeds(debug)
  uu.stdout_is_bytes(debug, debug_expected)
}

# origin: uutils test_sort::test_random_source_of_exactly_the_salt_length
test test_uu_sort_random_source_of_exactly_the_salt_length { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "source", b"xxxxxxxxxxxxxxxx")?
  uu.write(s, "input", "b\na\nc\n")?
  let first = uu.invoke(s, "sort", ["-R", "--random-source=source", "input"], timeout: 5s)?
  uu.succeeds(first)
  let second = uu.invoke(s, "sort", ["-R", "--random-source=source", "input"], timeout: 5s)?
  uu.succeeds(second)
  assert first.stdout == second.stdout
  assert first.stdout.utf8()?.lines().collect().len() == 3
}

# origin: uutils test_sort::test_random_source_shorter_than_the_salt
test test_uu_sort_random_source_shorter_than_the_salt { |ctx|
  for length in [0, 1, 15] {
    let s = uu.scene(ctx)?
    uu.write_bytes(s, "source", bytes.concat([b"x" for _ in range(length)]))?
    uu.write(s, "input", "b\na\nc\n")?
    let r = uu.invoke(s, "sort", ["-R", "--random-source=source", "input"], timeout: 5s)?
    uu.fails_with_code(r, 2)
    uu.no_stdout(r)
    uu.stderr_is(r, "sort: 'source': end of file\n")
  }
}

# origin: uutils test_sort::test_same_output_flag_twice_ok
test test_uu_sort_same_output_flag_twice_ok { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-o", "output", "-o", "output"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  uu.no_stderr(r)
}

# origin: uutils test_sort::test_same_sort_mode_twice
test test_uu_sort_same_sort_mode_twice { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "empty.txt", "empty.txt")?
  uu.succeeds(uu.invoke(s, "sort", ["-k", "2n,2n", "empty.txt"], timeout: 5s)?)
}

# origin: uutils test_sort::test_separator_attached_equals
test test_uu_sort_separator_attached_equals { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-t=", "-k", "2"], stdin: b"a=b=c\nb=a=d\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "b=a=d\na=b=c\n")
}

# origin: uutils test_sort::test_separator_null
test test_uu_sort_separator_null { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-k1,1", "-k3,3", "-t", "\\0"], stdin: b"z\0a\0b\nz\0b\0a\na\0z\0z\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a\0z\0z\nz\0b\0a\nz\0a\0b\n")
}

# origin: uutils test_sort::test_sigpipe_panic
test test_uu_sort_sigpipe_panic { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "ext_sort.txt", "ext_sort.txt")?
  uu.mkfifo(s, "stdout-pipe")?
  let reader = unix.open_fd(uu.at(s, "stdout-pipe"), nonblock: true)?
  var reader_open = true
  defer { if reader_open { unix.close_fd(reader)? } }
  let errors = uu.at(s, "stderr")
  let command = uu.command(s, "sort", ["ext_sort.txt"], stdout: uu.at(s, "stdout-pipe"), stderr: errors, timeout: 5s)?
  let child = spawn command?
  defer child.cancel(signal: "KILL", kill_after: 0ms)?
  unix.close_fd(reader)?
  reader_open = false
  let _ = wait child?
  assert errors.read_bytes()? == b""
}

# origin: uutils test_sort::test_sort_general_numeric_extremes
test test_uu_sort_sort_general_numeric_extremes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-g"], stdin: b"0\n-1.7976931348623157e+308\n1.7976931348623157e+308\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "-1.7976931348623157e+308\n0\n1.7976931348623157e+308\n")
}

# origin: uutils test_sort::test_sort_locale_punctuation
test test_uu_sort_sort_locale_punctuation { |ctx|
  let s = uu.scene(ctx)?
  for locale in ["en_US.UTF-8", "C"] {
    let r = uu.invoke(s, "sort", [], stdin: b"file10\nfile-10\n", vars: {LC_ALL: locale}, timeout: 5s)?
    uu.succeeds(r)
    uu.stdout_is(r, "file-10\nfile10\n")
  }
  for case in [
    {locale: "en_US.UTF-8", args: ["-u"]},
    {locale: "C", args: ["-u"]},
    {locale: "en_US.UTF-8", args: ["-u", "-k1,1"]},
    {locale: "en_US.UTF-8", args: ["-s", "-k1,1"]},
  ] {
    let r = uu.invoke(s, "sort", case.args, stdin: b"EU\nE.U\nE-U\n", vars: {LC_ALL: case.locale}, timeout: 5s)?
    uu.succeeds(r)
    uu.stdout_is(r, "E-U\nE.U\nEU\n")
  }
  for locale in ["en_US.UTF-8", "C"] {
    let r = uu.invoke(s, "sort", ["-u"], stdin: b"domain.com\n*.domain.com\ndomain.com\n", vars: {LC_ALL: locale}, timeout: 5s)?
    uu.succeeds(r)
    uu.stdout_is(r, "*.domain.com\ndomain.com\n")
  }
}

# origin: uutils test_sort::test_start_buffer
test test_uu_sort_start_buffer { |ctx|
  let s = uu.scene(ctx)?
  let large = bytes.concat([b"b" for _ in range(8000)])
  uu.write_bytes(s, "b", large)?
  uu.write_bytes(s, "a", b"aaa")?
  let r = uu.invoke(s, "sort", ["b", "a"], timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.concat([b"aaa\n", large, b"\n"]))
}

# origin: uutils test_sort::test_trailing_separator
test test_uu_sort_trailing_separator { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-t", "x", "-k", "1,1"], stdin: b"aax\naaa\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "aax\naaa\n")
}

# origin: uutils test_sort::test_unterminated_file_not_fused_across_chunk_boundary
test test_uu_sort_unterminated_file_not_fused_across_chunk_boundary { |ctx|
  for buffer_size in ["1b", "2b", "3b", "4b", "5b", "6b", "7b", "8b", "16b"] {
    let s = uu.scene(ctx)?
    uu.write(s, "first.txt", "a\nb\nc")?
    uu.write(s, "second.txt", "z\n")?
    let r = uu.invoke(s, "sort", ["-S", buffer_size, "first.txt", "second.txt"], timeout: 5s)?
    uu.succeeds(r)
    uu.stdout_only(r, "a\nb\nc\nz\n")
  }
}

# origin: uutils test_sort::test_verifies_files_after_keys
test test_uu_sort_verifies_files_after_keys { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-o", "nonexistent_dir/nonexistent_file", "-k", "0", "nonexistent_dir/input_file"], timeout: 5s)?
  uu.fails_with_code(r, 2)
  uu.stderr_contains(r, "invalid field specification '0'")
}

# origin: uutils test_sort::test_verifies_input_files
test test_uu_sort_verifies_input_files { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["/dev/random", "nonexistent_file"], timeout: 5s)?
  uu.fails_with_code(r, 2)
  uu.stderr_is(r, "sort: cannot read: nonexistent_file: No such file or directory\n")
}

# origin: uutils test_sort::test_verifies_input_files_without_opening_them
test test_uu_sort_verifies_input_files_without_opening_them { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "FIFO")?
  let r = uu.invoke(s, "sort", ["FIFO", "nonexistent_file"], timeout: 5s)?
  uu.fails_with_code(r, 2)
  uu.stderr_only(r, "sort: cannot read: nonexistent_file: No such file or directory\n")
}

# origin: uutils test_sort::test_verifies_out_file
test test_uu_sort_verifies_out_file { |ctx|
  for input in ["", "some input"] {
    let s = uu.scene(ctx)?
    let r = uu.invoke(s, "sort", ["-o", "nonexistent_dir/nonexistent_file"], stdin: bytes.from_text(input), timeout: 5s)?
    uu.fails_with_code(r, 2)
    uu.stderr_only(r, "sort: open failed: nonexistent_dir/nonexistent_file: No such file or directory\n")
  }
}

# origin: uutils test_sort::test_version
test test_uu_sort_version { |ctx|
  fixture_case(ctx, "version", ["-V"])?
}

# origin: uutils test_sort::test_version_empty_lines
test test_uu_sort_version_empty_lines { |ctx|
  fixture_case(ctx, "version-empty-lines", ["-V"])?
  fixture_case(ctx, "version-empty-lines", ["--version-sort"])?
}

# origin: uutils test_sort::test_version_sort_stable
test test_uu_sort_version_sort_stable { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["--stable", "--sort=version"], stdin: b"0.1\n0.02\n0.2\n0.002\n0.3\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "0.1\n0.02\n0.2\n0.002\n0.3\n")
}

# origin: uutils test_sort::test_version_sort_unstable
test test_uu_sort_version_sort_unstable { |ctx|
  let s = uu.scene(ctx)?
  for option in ["--sort=version", "--sort=versio", "--sort=v"] {
    let r = uu.invoke(s, "sort", [option], stdin: b"0.1\n0.02\n0.2\n0.002\n0.3\n", timeout: 5s)?
    uu.succeeds(r)
    uu.stdout_is(r, "0.1\n0.002\n0.02\n0.2\n0.3\n")
  }
}

# origin: uutils test_sort::test_words_unique
test test_uu_sort_words_unique { |ctx|
  fixture_case(ctx, "words_unique", ["-u"])?
}

# origin: uutils test_sort::test_wrong_args_exit_code
test test_uu_sort_wrong_args_exit_code { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["--misspelled"], timeout: 5s)?
  uu.fails_with_code(r, 2)
  uu.stderr_contains(r, "--misspelled")
}

# origin: uutils test_sort::test_zero_terminated
test test_uu_sort_zero_terminated { |ctx|
  fixture_case(ctx, "zero-terminated", ["-z"])?
}
