use support.uu

# origin: busybox grep/grep (default to stdin)
test test_bb_grep_grep_default_to_stdin_f1bfc1dd { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["two"], stdin: b"one\ntwo\nthree\nthree\nthree\n", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "two\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep (exit success)
test test_bb_grep_grep_exit_success_7f2ada07 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "source", "grep\n")?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["grep", "source"], stdin: b"", stdout: p"/dev/null", stderr: p"/dev/null", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep (exit with error)
test test_bb_grep_grep_exit_with_error_6c763c30 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["nonexistent"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
}

# origin: busybox grep/grep (no newline at EOL)
test test_bb_grep_grep_no_newline_at_EOL_de5ac559 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "bug")?
  let r = uu.invoke(s, "grep", ["bug", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "bug\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep - (specify stdin)
test test_bb_grep_grep_specify_stdin_cb77b890 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["two", "-"], stdin: b"one\ntwo\nthree\nthree\nthree\n", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "two\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep - infile (specify stdin and file)
test test_bb_grep_grep_infile_specify_stdin_and_file_7fd8db97 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one\ntwo\nthree\n")?
  let r = uu.invoke(s, "grep", ["two", "-", "input"], stdin: b"one\ntwo\ntoo\nthree\nthree\n", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "(standard input):two\ninput:two\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep - nofile (specify stdin and nonexisting file)
test test_bb_grep_grep_nofile_specify_stdin_and_nonexisting_file_a772026c { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["two", "-", "nonexistent"], stdin: b"one\ntwo\ntwo\nthree\nthree\nthree\n", timeout: 5s)?
  uu.fails_with_code(r, 2)
  uu.stdout_is(r, "(standard input):two\n(standard input):two\n")
}

# origin: busybox grep/grep -F -w w doesn't match ww
test test_bb_grep_grep_F_w_w_doesn_t_match_ww_72797a07 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "ww\n")?
  let r = uu.invoke(s, "grep", ["-F", "-w", "w", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -F handles -i
test test_bb_grep_grep_F_handles_i_172677b2 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "FOO\n")?
  let r = uu.invoke(s, "grep", ["-F", "-i", "foo", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "FOO\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -F handles multiple expessions
test test_bb_grep_grep_F_handles_multiple_expessions_58769cdd { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one\ntwo\n")?
  let r = uu.invoke(s, "grep", ["-F", "-e", "one", "-e", "two", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "one\ntwo\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -Fw doesn't stop on 1st mismatch
test test_bb_grep_grep_Fw_doesn_t_stop_on_1st_mismatch_3131d7ba { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "foop foo\n")?
  let r = uu.invoke(s, "grep", ["-Fw", "foo", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "foop foo\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -Fw matches only words
test test_bb_grep_grep_Fw_matches_only_words_2144e79f { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "foop\n")?
  let r = uu.invoke(s, "grep", ["-Fw", "foo", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -L exitcode 0
test test_bb_grep_grep_L_exitcode_0_5f6cb4b0 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "asd\n")?
  let r = uu.invoke(s, "grep", ["-L", "qwe", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "input\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -L exitcode 0 #2
test test_bb_grep_grep_L_exitcode_0_2_6779b1d1 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "qwe\n")?
  let r = uu.invoke(s, "grep", ["-L", "qwe", "input", "-"], stdin: b"asd\n", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "(standard input)\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -L exitcode 1
test test_bb_grep_grep_L_exitcode_1_259183c7 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "qwe\n")?
  let r = uu.invoke(s, "grep", ["-L", "qwe", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -e PATTERN can be a newline-delimited list
test test_bb_grep_grep_e_PATTERN_can_be_a_newline_delimited_list_ea4d4b65 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-Fv", "-e", "foo\nbar"], stdin: b"foo\nbar\nbaz\n", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "baz\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -f EMPTY_FILE
test test_bb_grep_grep_f_EMPTY_FILE_7af5c902 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-f", "input"], stdin: b"test\n", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -o does not loop forever
test test_bb_grep_grep_o_does_not_loop_forever_7a69a326 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-o", "[^/]*$"], stdin: b"/var/test\n", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "test\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -o does not loop forever on zero-length match
test test_bb_grep_grep_o_does_not_loop_forever_on_zero_length_match_76c0db6a { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-o", ""], stdin: b"test\n", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -q - nofile (specify stdin and nonexisting file, match)
test test_bb_grep_grep_q_nofile_specify_stdin_and_nonexisting_file_match_12fe03b3 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-q", "two", "-", "nonexistent"], stdin: b"one\ntwo\ntwo\nthree\nthree\nthree\n", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -q - nofile (specify stdin and nonexisting file, no match)
test test_bb_grep_grep_q_nofile_specify_stdin_and_nonexisting_file_no_match_e74f2187 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-q", "nomatch", "-", "nonexistent"], stdin: b"one\ntwo\ntwo\nthree\nthree\nthree\n", timeout: 5s)?
  uu.fails_with_code(r, 2)
  uu.stdout_is(r, "")
}

# origin: busybox grep/grep -r on dir/symlink to dir
test test_bb_grep_grep_r_on_dir_symlink_to_dir_23f340bd { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "grep.testdir/foo")?
  uu.write(s, "grep.testdir/foo/file", "bar\n")?
  uu.symlink(s, "foo", "grep.testdir/symfoo")?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-r", ".", "grep.testdir"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "grep.testdir/foo/file:bar\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -r on symlink to dir
test test_bb_grep_grep_r_on_symlink_to_dir_89b3b881 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "grep.testdir/foo")?
  uu.write(s, "grep.testdir/foo/file", "bar\n")?
  uu.symlink(s, "foo", "grep.testdir/symfoo")?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-r", ".", "grep.testdir/symfoo"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "grep.testdir/symfoo/file:bar\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -s nofile (nonexisting file, no match)
test test_bb_grep_grep_s_nofile_nonexisting_file_no_match_d76392a5 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-s", "nomatch", "nonexistent"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 2)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -s nofile - (stdin and nonexisting file, match)
test test_bb_grep_grep_s_nofile_stdin_and_nonexisting_file_match_8964118f { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-s", "domatch", "nonexistent", "-"], stdin: b"nomatch\ndomatch\nend\n", timeout: 5s)?
  uu.fails_with_code(r, 2)
  uu.stdout_is(r, "(standard input):domatch\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -v -f EMPTY_FILE
test test_bb_grep_grep_v_f_EMPTY_FILE_4e48c99a { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-v", "-f", "input"], stdin: b"test\n", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "test\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -vxf EMPTY_FILE
test test_bb_grep_grep_vxf_EMPTY_FILE_4d57e0b9 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-vxf", "input"], stdin: b"test\n", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "test\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -w ^ doesn't hang
test test_bb_grep_grep_w_doesn_t_hang_e030558d { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "anything\n")?
  let r = uu.invoke(s, "grep", ["-w", "^", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -w ^str doesn't match str not at the beginning
test test_bb_grep_grep_w_str_doesn_t_match_str_not_at_the_beginning_28444e7f { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "strstr\n")?
  let r = uu.invoke(s, "grep", ["-w", "^str", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -w doesn't stop on 1st mismatch
test test_bb_grep_grep_w_doesn_t_stop_on_1st_mismatch_cae47651 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "foop foo\n")?
  let r = uu.invoke(s, "grep", ["-w", "foo", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "foop foo\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -w word doesn't match wordword
test test_bb_grep_grep_w_word_doesn_t_match_wordword_77f228e1 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "wordword\n")?
  let r = uu.invoke(s, "grep", ["-w", "word", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -w word match second word
test test_bb_grep_grep_w_word_match_second_word_1ed0fd9c { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "bword,word\nwordb,word\nbwordb,word\n")?
  let r = uu.invoke(s, "grep", ["-w", "word", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "bword,word\nwordb,word\nbwordb,word\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -x (full match)
test test_bb_grep_grep_x_full_match_e3810778 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "foo\n")?
  let r = uu.invoke(s, "grep", ["-x", "foo", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "foo\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -x (partial match 1)
test test_bb_grep_grep_x_partial_match_1_480b88b9 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "foo bar\n")?
  let r = uu.invoke(s, "grep", ["-x", "foo", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -x (partial match 2)
test test_bb_grep_grep_x_partial_match_2_f717eed5 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "bar foo\n")?
  let r = uu.invoke(s, "grep", ["-x", "foo", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -x -F (full match)
test test_bb_grep_grep_x_F_full_match_4cda351b { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "foo\n")?
  let r = uu.invoke(s, "grep", ["-x", "-F", "foo", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "foo\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -x -F (partial match 1)
test test_bb_grep_grep_x_F_partial_match_1_5af0bb42 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "foo bar\n")?
  let r = uu.invoke(s, "grep", ["-x", "-F", "foo", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -x -F (partial match 2)
test test_bb_grep_grep_x_F_partial_match_2_8a93c142 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "bar foo\n")?
  let r = uu.invoke(s, "grep", ["-x", "-F", "foo", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep -x -v -e EXP1 -e EXP2 finds nothing if either EXP matches
test test_bb_grep_grep_x_v_e_EXP1_e_EXP2_finds_nothing_if_either_EXP_matches_951af571 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-x", "-v", "-e", ".*aa.*", "-e", "bb.*"], stdin: b"  aa bb cc\n", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
}

# origin: busybox grep/grep PATTERN can be a newline-delimited list
test test_bb_grep_grep_PATTERN_can_be_a_newline_delimited_list_a0aac618 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "grep", ["-Fv", "foo\nbar"], stdin: b"foo\nbar\nbaz\n", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "baz\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep can read regexps from stdin
test test_bb_grep_grep_can_read_regexps_from_stdin_81a58b83 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "tw\ntwo\nthree\n")?
  let r = uu.invoke(s, "grep", ["-f", "-", "input"], stdin: b"tw.\nthr\n", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "two\nthree\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep handles multiple regexps
test test_bb_grep_grep_handles_multiple_regexps_077d07d6 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one\ntwo\n")?
  let r = uu.invoke(s, "grep", ["-e", "one", "-e", "two", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "one\ntwo\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep input (specify file)
test test_bb_grep_grep_input_specify_file_7504ab2f { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one\ntwo\nthree\nthree\nthree\n")?
  let r = uu.invoke(s, "grep", ["two", "input"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "two\n")
  uu.no_stderr(r)
}

# origin: busybox grep/grep two files
test test_bb_grep_grep_two_files_2cc08cfe { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "empty")?
  uu.write(s, "input", "one\ntwo\nthree\nthree\nthree\n")?
  let r = uu.invoke(s, "grep", ["two", "input", "empty"], stdin: b"", timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "input:two\n")
  uu.no_stderr(r)
}

