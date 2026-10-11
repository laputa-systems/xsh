use support.uu as uu

# origin: busybox nl/nl numbers all lines
test test_bb_nl_nl_numbers_all_lines_d8c243ca { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "line 1\n\nline 3\n")?
  let r = uu.invoke(s, "nl", ["-b", "a", "input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "     1\tline 1\n     2\t\n     3\tline 3\n")
}

# origin: busybox nl/nl numbers no lines
test test_bb_nl_nl_numbers_no_lines_67272eae { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "line 1\n\nline 3\n")?
  let r = uu.invoke(s, "nl", ["-b", "n", "input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "       line 1\n       \n       line 3\n")
}

# origin: busybox nl/nl numbers non-empty lines
test test_bb_nl_nl_numbers_non_empty_lines_0ef71f23 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "line 1\n\nline 3\n")?
  let r = uu.invoke(s, "nl", ["-b", "t", "input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "     1\tline 1\n       \n     2\tline 3\n")
}

