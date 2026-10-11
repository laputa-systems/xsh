use support.uu as uu

# origin: uutils test_head::test_stdin_default
test test_uu_head_stdin_default { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", [], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_default.expected".read_bytes()?)

}

# origin: uutils test_head::test_stdin_1_line_obsolete
test test_uu_head_stdin_1_line_obsolete { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["-1"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_1_line.expected".read_bytes()?)

}

# origin: uutils test_head::test_stdin_1_line
test test_uu_head_stdin_1_line { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["-n", "1"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_1_line.expected".read_bytes()?)

}

# origin: uutils test_head::test_stdin_negative_23_line
test test_uu_head_stdin_negative_23_line { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["-n", "-23"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_1_line.expected".read_bytes()?)

}

# origin: uutils test_head::test_stdin_5_chars
test test_uu_head_stdin_5_chars { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["-c", "5"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_5_chars.expected".read_bytes()?)

}

# origin: uutils test_head::test_single_default
test test_uu_head_single_default { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["lorem_ipsum.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_default.expected".read_bytes()?)

}

# origin: uutils test_head::test_single_1_line_obsolete
test test_uu_head_single_1_line_obsolete { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["-1", "lorem_ipsum.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_1_line.expected".read_bytes()?)

}

# origin: uutils test_head::test_single_1_line
test test_uu_head_single_1_line { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["-n", "1", "lorem_ipsum.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_1_line.expected".read_bytes()?)

}

# origin: uutils test_head::test_single_1_line_presume_input_pipe
test test_uu_head_single_1_line_presume_input_pipe { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["---presume-input-pipe", "-n", "1", "lorem_ipsum.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_1_line.expected".read_bytes()?)

}

# origin: uutils test_head::test_single_5_chars
test test_uu_head_single_5_chars { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["-c", "5", "lorem_ipsum.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_5_chars.expected".read_bytes()?)

}

# origin: uutils test_head::test_verbose
test test_uu_head_verbose { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["-v", "lorem_ipsum.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_verbose.expected".read_bytes()?)

}

# origin: uutils test_head::test_presume_input_pipe_default
test test_uu_head_presume_input_pipe_default { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["---presume-input-pipe"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_default.expected".read_bytes()?)

}

# origin: uutils test_head::test_presume_input_pipe_5_chars
test test_uu_head_presume_input_pipe_5_chars { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["-c", "5", "---presume-input-pipe"], stdin: uu.read(s, "lorem_ipsum.txt")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_5_chars.expected".read_bytes()?)

}

# origin: uutils test_head::test_file_backwards
test test_uu_head_file_backwards { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["-c", "-10", "lorem_ipsum.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_backwards_file.expected".read_bytes()?)

}

# origin: uutils test_head::test_all_but_last_lines
test test_uu_head_all_but_last_lines { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["-n", "-15", "lorem_ipsum.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/lorem_ipsum_backwards_15_lines.expected".read_bytes()?)

}

# origin: uutils test_head::test_spams_newline
test test_uu_head_spams_newline { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", [], stdin: b"a")?
  uu.succeeds(r)
  uu.stdout_is(r, "a")

}

# origin: uutils test_head::test_byte_syntax
test test_uu_head_byte_syntax { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-1c"], stdin: b"abc")?
  uu.succeeds(r)
  uu.stdout_is(r, "a")

}

# origin: uutils test_head::test_line_syntax
test test_uu_head_line_syntax { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-n", "2048m"], stdin: b"a\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "a\n")

}

# origin: uutils test_head::test_zero_terminated_syntax
test test_uu_head_zero_terminated_syntax { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-z", "-n", "1"], stdin: b"x\0y")?
  uu.succeeds(r)
  uu.stdout_is(r, "x\0")

}

# origin: uutils test_head::test_zero_terminated_syntax_2
test test_uu_head_zero_terminated_syntax_2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-z", "-n", "2"], stdin: b"x\0y")?
  uu.succeeds(r)
  uu.stdout_is(r, "x\0y")

}

# origin: uutils test_head::test_non_terminated_input
test test_uu_head_non_terminated_input { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-n", "-1"], stdin: b"x\ny")?
  uu.succeeds(r)
  uu.stdout_is(r, "x\n")

}

# origin: uutils test_head::test_zero_terminated_negative_lines
test test_uu_head_zero_terminated_negative_lines { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-z", "-n", "-1"], stdin: b"x\0y\0z\0")?
  uu.succeeds(r)
  uu.stdout_is(r, "x\0y\0")

}

# origin: uutils test_head::test_negative_byte_syntax
test test_uu_head_negative_byte_syntax { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["--bytes=-2"], stdin: b"a\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "")

}

# origin: uutils test_head::test_negative_bytes_greater_than_input_size_stdin
test test_uu_head_negative_bytes_greater_than_input_size_stdin { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-c", "-2"], stdin: b"a")?
  uu.succeeds(r)
  uu.stdout_is(r, "")

}

# origin: uutils test_head::test_negative_zero_bytes
test test_uu_head_negative_zero_bytes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["--bytes=-0"], stdin: b"qwerty")?
  uu.succeeds(r)
  uu.stdout_is(r, "qwerty")

}

# origin: uutils test_head::test_lines_leading_zeros
test test_uu_head_lines_leading_zeros { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["--lines=010"], stdin: b"\n\n\n\n\n\n\n\n\n\n\n\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "\n\n\n\n\n\n\n\n\n\n")

}

# origin: uutils test_head::test_obsolete_extras
test test_uu_head_obsolete_extras { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-5zv"], stdin: b"1\02\03\04\05\06")?
  uu.succeeds(r)
  uu.stdout_is(r, "==> 'standard input' <==\n1\02\03\04\05\0")

}

# origin: uutils test_head::test_negative_zero_lines
test test_uu_head_negative_zero_lines { |ctx|
  let s = uu.scene(ctx)?
{
  let r = uu.invoke(s, "head", ["--lines=-0"], stdin: b"a\nb\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "a\nb\n")
}
{
  let r = uu.invoke(s, "head", ["--lines=-0"], stdin: b"a\nb")?
  uu.succeeds(r)
  uu.stdout_is(r, "a\nb")
}
}

# origin: uutils test_head::test_negative_bytes_greater_than_input_size_file
test test_uu_head_negative_bytes_greater_than_input_size_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "a")?
  let r = uu.invoke(s, "head", ["-c", "-2", "f"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "")
  uu.no_stderr(r)

}

# origin: uutils test_head::test_zero_bytes_with_suffix
test test_uu_head_zero_bytes_with_suffix { |ctx|
  let s = uu.scene(ctx)?
  for count in ["0K", "00K", "+0K"] {
  let r = uu.invoke(s, "head", [f"--bytes={count}"], stdin: b"qwerty")?
  uu.succeeds(r)
  uu.no_output(r)
  }
}

# origin: uutils test_head::test_invalid_arg
test test_uu_head_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["--definitely-invalid"], stdin: b"")?
  uu.fails_with_code(r, 1)

}

# origin: uutils test_head::test_no_such_file_or_directory
test test_uu_head_no_such_file_or_directory { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["no_such_file.toml"], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "cannot open 'no_such_file.toml' for reading: No such file or directory")

}

# origin: uutils test_head::test_multiple_nonexistent_files
test test_uu_head_multiple_nonexistent_files { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["bogusfile1", "bogusfile2"], stdin: b"")?
  uu.fails(r)
  assert !("==> bogusfile1 <==" in r.stdout.utf8()?)
  assert !("==> bogusfile2 <==" in r.stdout.utf8()?)
  uu.stderr_contains(r, "cannot open 'bogusfile1' for reading: No such file or directory")
  uu.stderr_contains(r, "cannot open 'bogusfile2' for reading: No such file or directory")

}

# origin: uutils test_head::test_sequence_fixture
test test_uu_head_sequence_fixture { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "sequence", "sequence")?
  let r = uu.invoke(s, "head", ["-n", "-10", "sequence"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/sequence.expected".read_bytes()?)

}

# origin: uutils test_head::test_zero_terminated
test test_uu_head_zero_terminated { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "zero_terminated.txt", "zero_terminated.txt")?
  let r = uu.invoke(s, "head", ["-z", "zero_terminated.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/head/zero_terminated.expected".read_bytes()?)

}

# origin: uutils test_head::test_multiple_files
test test_uu_head_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "emptyfile.txt")?
  let r = uu.invoke(s, "head", ["emptyfile.txt", "emptyfile.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "==> emptyfile.txt <==\n\n==> emptyfile.txt <==\n")

}

# origin: uutils test_head::test_multiple_files_with_stdin
test test_uu_head_multiple_files_with_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "emptyfile.txt")?
  let r = uu.invoke(s, "head", ["emptyfile.txt", "-", "emptyfile.txt"], stdin: b"hello\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "==> emptyfile.txt <==\n\n==> 'standard input' <==\nhello\n\n==> emptyfile.txt <==\n")

}

# origin: uutils test_head::test_bad_utf8
test test_uu_head_bad_utf8 { |ctx|
  let s = uu.scene(ctx)?
  let data = b"\xfc\x80\x80\x80\x80\xaf"
  let r = uu.invoke(s, "head", ["-c", "6"], stdin: data)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, data)

}

# origin: uutils test_head::test_bad_utf8_lines
test test_uu_head_bad_utf8_lines { |ctx|
  let s = uu.scene(ctx)?
  let data = b"\xfc\x80\x80\x80\x80\xaf"
  let r = uu.invoke(s, "head", ["-n", "2"], stdin: bytes.concat([data, b"\nb", data, b"\nb", data]))?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.concat([data, b"\nb", data, b"\n"]))

}

# origin: uutils test_head::test_head_num_with_undocumented_sign_bytes
test test_uu_head_head_num_with_undocumented_sign_bytes { |ctx|
  let s = uu.scene(ctx)?
{
  let r = uu.invoke(s, "head", ["-c", "5"], stdin: b"abcdefghijklmnopqrstuvwxyz")?
  uu.succeeds(r)
  uu.stdout_is(r, "abcde")
}
{
  let r = uu.invoke(s, "head", ["-c", "-5"], stdin: b"abcdefghijklmnopqrstuvwxyz")?
  uu.succeeds(r)
  uu.stdout_is(r, "abcdefghijklmnopqrstu")
}
{
  let r = uu.invoke(s, "head", ["-c", "+5"], stdin: b"abcdefghijklmnopqrstuvwxyz")?
  uu.succeeds(r)
  uu.stdout_is(r, "abcde")
}
}

# origin: uutils test_head::test_all_but_last_bytes_large_file_piped
test test_uu_head_all_but_last_bytes_large_file_piped { |ctx|
  let s = uu.scene(ctx)?
  let input = [f"{i}\n" for i in range(1, 20001)].join("")
  let expected = [f"{i}\n" for i in range(1, 19001)].join("")
  let tail = [f"{i}\n" for i in range(19001, 20001)].join("")
  let r = uu.invoke(s, "head", ["-c", f"-{bytes.from_text(tail).len()}"], stdin: bytes.from_text(input))?
  uu.succeeds(r)
  uu.stdout_only(r, expected)
}

# origin: uutils test_head::test_all_but_last_lines_large_file_presume_input_pipe
test test_uu_head_all_but_last_lines_large_file_presume_input_pipe { |ctx|
  let s = uu.scene(ctx)?
  let input = ["aaaaaa\n" for _ in range(20000)].join("")
  uu.write(s, "reused_line_chunks", input)?
  let r = uu.invoke(s, "head", ["---presume-input-pipe", "-n", "-1", "reused_line_chunks"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, ["aaaaaa\n" for _ in range(19999)].join(""))
}

# origin: uutils test_head::test_all_but_last_lines_large_file
test test_uu_head_all_but_last_lines_large_file { |ctx|
  let s = uu.scene(ctx)?
  let input = [f"{i}\n" for i in range(1, 20001)].join("")
  uu.write(s, "seq_20000", input)?
  let truncated = uu.invoke(s, "head", ["-c", "-1", "seq_20000"])?
  uu.succeeds(truncated)
  uu.write_bytes(s, "seq_20000_truncated", truncated.stdout)?
  let expected = [f"{i}\n" for i in range(1, 1001)].join("")
  for file in ["seq_20000", "seq_20000_truncated"] {
    for count in ["-19000", "-20000", "-20001"] {
      let r = uu.invoke(s, "head", ["-n", count, file])?
      uu.succeeds(r)
      uu.stdout_only(r, if count == "-19000" { expected } else { "" })
    }
  }
}

# origin: uutils test_head::test_read_backwards_bytes_proc_fs_version
test test_uu_head_read_backwards_bytes_proc_fs_version { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-c", "-1", "/proc/version"], stdin: b"")?
  uu.succeeds(r)
  assert r.stdout.len() > 0
}

# origin: uutils test_head::test_read_backwards_bytes_proc_fs_modules
test test_uu_head_read_backwards_bytes_proc_fs_modules { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-c", "-1", "/proc/modules"], stdin: b"")?
  uu.succeeds(r)
  if p"/proc/modules".read_bytes()?.len() > 0 { assert r.stdout.len() > 0 }
}

# origin: uutils test_head::test_read_backwards_lines_proc_fs_modules
test test_uu_head_read_backwards_lines_proc_fs_modules { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["--lines", "-1", "/proc/modules"], stdin: b"")?
  uu.succeeds(r)
  if p"/proc/modules".read_bytes()?.len() > 0 { assert r.stdout.len() > 0 }
}

# origin: uutils test_head::test_read_backwards_bytes_sys_kernel_profiling
test test_uu_head_read_backwards_bytes_sys_kernel_profiling { |ctx|
  let s = uu.scene(ctx)?
  if p"/sys/kernel/profiling".is_file()? {
  let r = uu.invoke(s, "head", ["-c", "-1", "/sys/kernel/profiling"], stdin: b"")?
  uu.succeeds(r)
  assert r.stdout == b"0" or r.stdout == b"1"
  }
}

# origin: uutils test_head::test_value_too_large
test test_uu_head_value_too_large { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r = uu.invoke(s, "head", ["-n", "184467440737095516150", "lorem_ipsum.txt"], stdin: b"")?
  uu.succeeds(r)

}

# origin: uutils test_head::test_write_to_dev_full
test test_uu_head_write_to_dev_full { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "head", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  for append in [true, false] {
    let r = uu.invoke(s, "head", [], stdin: uu.read(s, "lorem_ipsum.txt")?, stdout: p"/dev/full", stdout_append: append)?
    uu.fails(r)
    uu.stderr_is(r, "head: write error: No space left on device\n")
  }
}

# origin: uutils test_head::test_head_non_utf8_paths
test test_uu_head_head_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let file_path = uu.at_bytes(s, b"test_\xff\xfe.txt")?
  file_path.write("line1\nline2\nline3\n")?
  let r = uu.invoke_paths(s, "head", [file_path])?
  uu.succeeds(r)
  uu.stdout_contains(r, "line1")
  uu.stdout_contains(r, "line2")
  uu.stdout_contains(r, "line3")
}

# origin: uutils test_head::test_do_not_attempt_to_read_a_directory
test test_uu_head_do_not_attempt_to_read_a_directory { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["."], stdin: b"")?
  uu.fails(r)
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "error reading '.'")
}

# origin: uutils test_head::test_zero_bytes_on_directory_succeeds
test test_uu_head_zero_bytes_on_directory_succeeds { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-c", "0", "."], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: uutils test_head::test_zero_lines_on_directory_succeeds
test test_uu_head_zero_lines_on_directory_succeeds { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-n", "0", "."], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: uutils test_head::test_directory_header_with_multiple_files
test test_uu_head_directory_header_with_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.write(s, "f", "hello\n")?
  let r = uu.invoke(s, "head", ["-c", "5", "d", "f"], stdin: b"")?
  uu.fails(r)
  uu.stdout_is(r, "==> d <==\n\n==> f <==\nhello")
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "Is a directory")
}

# origin: uutils test_head::test_directory_header_with_multiple_files_zero_output
test test_uu_head_directory_header_with_multiple_files_zero_output { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.write(s, "f", "hello\n")?
  let r = uu.invoke(s, "head", ["-c", "0", "d", "f"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "==> d <==\n\n==> f <==\n")
  uu.no_stderr(r)
}

# origin: uutils test_head::test_unreadable_file_prints_no_header
test test_uu_head_unreadable_file_prints_no_header { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "unreadable", "secret\n")?
  uu.write(s, "readable", "hello\n")?
  uu.set_mode(s, "unreadable", 0o000)?
  let r = uu.invoke(s, "head", ["-c", "5", "unreadable", "readable"], stdin: b"")?
  uu.fails(r)
  uu.stdout_is(r, "==> readable <==\nhello")
  uu.fails_with_code(r, 1)
  assert !("==> unreadable <==" in r.stdout.utf8()?)
  uu.stderr_contains(r, "cannot open 'unreadable' for reading: Permission denied")
}

# origin: uutils test_head::test_verbose_header_write_error_long_filename
test test_uu_head_verbose_header_write_error_long_filename { |ctx|
  let s = uu.scene(ctx)?
  let long = "/dev/" + ["./" for _ in range(512)].join("") + "null"
  let r = uu.invoke(s, "head", ["-v", long], stdout: p"/dev/full")?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "No space left on device")
}

# origin: uutils test_head::test_head_rejects_directory_through_symlink
test test_uu_head_head_rejects_directory_through_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "real_dir")?
  uu.symlink(s, "real_dir", "link_to_dir")?
  let r = uu.invoke(s, "head", ["link_to_dir"], stdin: b"")?
  uu.fails(r)
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "Is a directory")
}

# origin: uutils test_head::test_head_follows_symlink_to_regular_file
test test_uu_head_head_follows_symlink_to_regular_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "regular", "hello\n")?
  uu.symlink(s, "regular", "link_to_regular")?
  let r = uu.invoke(s, "head", ["link_to_regular"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "hello\n")

}

# origin: uutils test_head::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_head_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "head", ["-c", "1fb", "/dev/null"], stdin: b"")?
  uu.fails(r)
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "head: invalid number of bytes: '1fb'\n")
}

# origin: uutils test_head::test_invalid_count_keeps_its_leading_zeros
test test_uu_head_invalid_count_keeps_its_leading_zeros { |ctx|
  let s = uu.scene(ctx)?
{
  let r = uu.invoke(s, "head", ["-c", "0fb", "/dev/null"], stdin: b"")?
  uu.fails(r)
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "head: invalid number of bytes: '0fb'\n")
}
{
  let r = uu.invoke(s, "head", ["-n", "00x", "/dev/null"], stdin: b"")?
  uu.fails(r)
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "head: invalid number of lines: '00x'\n")
}
}

# origin: uutils test_head::test_lowercase_multiplier_suffixes_rejected
test test_uu_head_lowercase_multiplier_suffixes_rejected { |ctx|
  let s = uu.scene(ctx)?
  for suffix in ["g", "t", "p", "e", "z", "y", "r", "q"] {
    for pair in [["-c", "bytes"], ["-n", "lines"]] {
      let r = uu.invoke(s, "head", [pair[0], f"2{suffix}"])?
      uu.fails_with_code(r, 1)
      uu.stderr_is(r, f"head: invalid number of {pair[1]}: '2{suffix}'\n")
    }
  }
}

# origin: uutils test_head::test_accepted_multiplier_suffixes
test test_uu_head_accepted_multiplier_suffixes { |ctx|
  let s = uu.scene(ctx)?
  for suffix in ["b", "k", "m", "K", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q", "kB", "KiB", "kD", "MiB", "GB"] {
    let r = uu.invoke(s, "head", ["-c", f"1{suffix}"], stdin: b"x")?
    uu.succeeds(r)
  }
}

# origin: uutils test_head::test_header_quotes_names_needing_it
test test_uu_head_header_quotes_names_needing_it { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "plain", "p\n")?
  uu.write(s, "two words", "w\n")?
  let r = uu.invoke(s, "head", ["-n1", "plain", "two words"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "==> plain <==\np\n\n==> 'two words' <==\nw\n")
  uu.no_stderr(r)
}

# origin: uutils test_head::test_header_quotes_name_with_control_char
test test_uu_head_header_quotes_name_with_control_char { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "tab\there", "t\n")?
  let r = uu.invoke(s, "head", ["-v", "-n1", "tab\there"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "==> 'tab'$'\\t''here' <==\nt\n")
  uu.no_stderr(r)
}

