use support.uu as uu

# origin: busybox sed/sed special char as s/// delimiter, in replacement 1
test test_bb_sed_sed_special_char_as_s_delimiter_in_replacement_1_1ee3de3e { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s&9&X\\&&"], stdin: bytes.from_text("9+8=17\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "X&+8=17\n")
}

# origin: busybox sed/sed special char as s/// delimiter, in replacement 2
test test_bb_sed_sed_special_char_as_s_delimiter_in_replacement_2_dbdaf61d { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s1\\(9\\)1X\\11"], stdin: bytes.from_text("9+8=17\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "X1+8=17\n")
}

# origin: busybox sed/sed stdin twice
test test_bb_sed_sed_stdin_twice_7a1b962e { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["", "-", "-"], stdin: bytes.from_text("hello"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "hello")
}

# origin: busybox sed/sed subst+write
test test_bb_sed_sed_subst_write_afa9dd89 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "thingy")?
  let r = uu.invoke(s, "sed", ["-e", "s/i/z/", "-e", "woutputw", "input", "-"], stdin: bytes.from_text("again"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "thzngy\nagazn")
  uu.file_is(s, "outputw", "thzngy\nagazn")
}

# origin: busybox sed/sed t (test/branch clears test bit)
test test_bb_sed_sed_t_test_branch_clears_test_bit_f8563e80 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/a/b/;:loop;t loop"], stdin: bytes.from_text("a\nb\nc\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "b\nb\nc\n")
}

# origin: busybox sed/sed t (test/branch)
test test_bb_sed_sed_t_test_branch_d349ccbb { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/a/1/;t one;p;: one;p"], stdin: bytes.from_text("a\nb\nc\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "1\n1\nb\nb\nb\nc\nc\nc\n")
}

# origin: busybox sed/sed trailing NUL
test test_bb_sed_sed_trailing_NUL_eec140c3 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "a\0b\0")?
  let r = uu.invoke(s, "sed", ["s/i/z/", "input", "-"], stdin: bytes.from_text("c"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "a\0b\0\nc")
}

# origin: busybox sed/sed understands \r
test test_bb_sed_sed_understands_r_1795a75c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s/r/\\r/"], stdin: bytes.from_text("rrr\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "\rrr\n")
}

# origin: busybox sed/sed understands duplicate file name
test test_bb_sed_sed_understands_duplicate_file_name_fa19fd60 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-n", "-e", "/a/w sed.output", "-e", "/c/w sed.output"], stdin: bytes.from_text("a\nb\nc\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "")
  uu.no_stderr(r)
  uu.file_is(s, "sed.output", "a\nc\n")
}

# origin: busybox sed/sed uses previous regexp
test test_bb_sed_sed_uses_previous_regexp_f6826fce { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["/w/p;//q"], stdin: bytes.from_text("q\nw\ne\nr\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "q\nw\nw\n")
}

# origin: busybox sed/sed with N skipping lines past ranges on next cmds
test test_bb_sed_sed_with_N_skipping_lines_past_ranges_on_next_cmds_85a3c578 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-n", "1{N;N;d};1p;2,3p;3p;4p"], stdin: bytes.from_text("1\n2\n3\n4\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "4\n4\n")
}

# origin: busybox sed/sed with empty match
test test_bb_sed_sed_with_empty_match_b2218bdb { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s/z*//g"], stdin: bytes.from_text("string\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "string\n")
}

# origin: busybox sed/sed zero chars match/replace advances correctly 1
test test_bb_sed_sed_zero_chars_match_replace_advances_correctly_1_d4b74b08 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s/l*/@/g"], stdin: bytes.from_text("helllo\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "@h@e@o@\n")
}

# origin: busybox sed/sed zero chars match/replace advances correctly 2
test test_bb_sed_sed_zero_chars_match_replace_advances_correctly_2_1ffe4949 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s [^ .]* x g"], stdin: bytes.from_text(" a.b\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "x x.x\n")
}

# origin: busybox sed/sed zero chars match/replace logic must not falsely trigger here 1
test test_bb_sed_sed_zero_chars_match_replace_logic_must_not_falsely_trigger_here_1_a199c1a1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s/a/A/g"], stdin: bytes.from_text("_aaa1aa\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "_AAA1AA\n")
}

# origin: busybox sed/sed zero chars match/replace logic must not falsely trigger here 2
test test_bb_sed_sed_zero_chars_match_replace_logic_must_not_falsely_trigger_here_2_241ef804 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s/ *$/_/g"], stdin: bytes.from_text("qwerty\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_is(r, "qwerty_\n")
}

