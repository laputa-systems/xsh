use support.uu as uu

# origin: busybox rev/rev file with long line
test test_bb_rev_rev_file_with_long_line_9bc052fb { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+--------------+\nabc\n")?
  let r = uu.invoke(s, "rev", ["input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "+--------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------+---------------\ncba\n")
}

# origin: busybox rev/rev file with missing newline
test test_bb_rev_rev_file_with_missing_newline_f8d2ada0 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "line 1\n\nline 3")?
  let r = uu.invoke(s, "rev", ["input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1 enil\n\n3 enil")
}

# origin: busybox rev/rev works
test test_bb_rev_rev_works_1bcfd0bf { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "line 1\n\nline 3\n")?
  let r = uu.invoke(s, "rev", ["input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1 enil\n\n3 enil\n")
}

