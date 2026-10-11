use support.uu

# origin: busybox uniq/uniq (exit with error)
test test_bb_uniq_uniq_exit_with_error_6b67b52b { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["nonexistent"], stdin: b"")?
  uu.fails(r)
  uu.no_stdout(r)
}

# origin: busybox uniq/uniq (exit success)
test test_bb_uniq_uniq_exit_success_d6aeffbf { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["/dev/null"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: busybox uniq/uniq (default to stdin)
test test_bb_uniq_uniq_default_to_stdin_ab41006f { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", [], stdin: b"one\ntwo\ntwo\nthree\nthree\nthree\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "one\ntwo\nthree\n")
}

# origin: busybox uniq/uniq - (specify stdin)
test test_bb_uniq_uniq_specify_stdin_97f3b1fa { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-"], stdin: b"one\ntwo\ntwo\nthree\nthree\nthree\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "one\ntwo\nthree\n")
}

# origin: busybox uniq/uniq input (specify file)
test test_bb_uniq_uniq_input_specify_file_3a31c1af { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one\ntwo\ntwo\nthree\nthree\nthree\n")?
  let r = uu.invoke(s, "uniq", ["input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "one\ntwo\nthree\n")
}

# origin: busybox uniq/uniq input outfile (two files)
test test_bb_uniq_uniq_input_outfile_two_files_a10dab1e { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one\ntwo\ntwo\nthree\nthree\nthree\n")?
  let r = uu.invoke(s, "uniq", ["input", "actual"])?
  uu.succeeds(r)
  uu.no_output(r)
  uu.file_is(s, "actual", "one\ntwo\nthree\n")
}

# origin: busybox uniq/uniq (stdin) outfile
test test_bb_uniq_uniq_stdin_outfile_fc36d68b { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-", "actual"], stdin: b"one\ntwo\ntwo\nthree\nthree\nthree\n")?
  uu.succeeds(r)
  uu.no_output(r)
  uu.file_is(s, "actual", "one\ntwo\nthree\n")
}

# origin: busybox uniq/uniq input - (specify stdout)
test test_bb_uniq_uniq_input_specify_stdout_3ea60572 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one\ntwo\ntwo\nthree\nthree\nthree\n")?
  let r = uu.invoke(s, "uniq", ["input", "-"])?
  uu.succeeds(r)
  uu.stdout_only(r, "one\ntwo\nthree\n")
}

# origin: busybox uniq/uniq -c (occurrence count)
test test_bb_uniq_uniq_c_occurrence_count_3e4b4a5e { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-c"], stdin: b"one\ntwo\ntwo\nthree\nthree\nthree\n")?
  uu.succeeds(r)
  uu.no_stderr(r)
  let lines = r.stdout.utf8()?.split("\n")
  assert [line.trim() for line in lines] == ["1 one", "2 two", "3 three", ""]
}

# origin: busybox uniq/uniq -d (dups only)
test test_bb_uniq_uniq_d_dups_only_83db94a5 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-d"], stdin: b"one\ntwo\ntwo\nthree\nthree\nthree\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "two\nthree\n")
}

# origin: busybox uniq/uniq -f -s (skip fields and chars)
test test_bb_uniq_uniq_f_s_skip_fields_and_chars_ba2c0aaf { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-f2", "-s", "3"], stdin: b"cc\tdd\tee8\nbb\tcc\tdd8\naa\tbb\tcc9\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "cc\tdd\tee8\naa\tbb\tcc9\n")
}

# origin: busybox uniq/uniq -w (compare max characters)
test test_bb_uniq_uniq_w_compare_max_characters_0dfe30df { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-w", "2"], stdin: b"cc1\ncc2\ncc3\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "cc1\n")
}

# origin: busybox uniq/uniq -s -w (skip fields and compare max chars)
test test_bb_uniq_uniq_s_w_skip_fields_and_compare_max_chars_43b1cc84 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-s", "2", "-w", "2"], stdin: b"aaccaa\naaccbb\nbbccaa\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "aaccaa\n")
}

# origin: busybox uniq/uniq -u and -d produce no output
test test_bb_uniq_uniq_u_and_d_produce_no_output_a3b8181d { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uniq", ["-d", "-u"], stdin: b"one\ntwo\ntwo\nthree\nthree\nthree\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

