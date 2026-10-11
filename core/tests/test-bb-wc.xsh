use support.uu as uu

# origin: busybox wc/wc-counts-all
test test_bb_wc_wc_counts_all_b846225a { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "wc", [], stdin: b"i'm a little teapot\n")?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert [part for part in r.stdout.utf8()?.trim().split(" ") if part != ""] == ["1", "4", "20"]
}

# origin: busybox wc/wc-counts-characters
test test_bb_wc_wc_counts_characters_ece4b91a { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "wc", ["-c"], stdin: b"i'm a little teapot\n")?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.trim().parse_int()? == 20
}

# origin: busybox wc/wc-counts-lines
test test_bb_wc_wc_counts_lines_5cf91172 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "wc", ["-l"], stdin: b"i'm a little teapot\n")?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.trim().parse_int()? == 1
}

# origin: busybox wc/wc-counts-words
test test_bb_wc_wc_counts_words_66df776b { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "wc", ["-w"], stdin: b"i'm a little teapot\n")?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.trim().parse_int()? == 4
}

# origin: busybox wc/wc-prints-longest-line-length
test test_bb_wc_wc_prints_longest_line_length_24347826 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "wc", ["-L"], stdin: b"i'm a little teapot\n")?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.trim().parse_int()? == 19
}

