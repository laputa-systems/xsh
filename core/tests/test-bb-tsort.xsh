use support.uu

# origin: busybox tsort/tsort
test test_bb_tsort_tsort_0ade2139 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: b"a a\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
}

# origin: busybox tsort/tsort -
test test_bb_tsort_tsort_b49211a2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tsort", ["-"], stdin: b"a a\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
}

# origin: busybox tsort/tsort input
test test_bb_tsort_tsort_input_cb257b29 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "a a\n")?
  let r = uu.invoke(s, "tsort", ["input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
}

# origin: busybox tsort/tsort input (w/o eol)
test test_bb_tsort_tsort_input_w_o_eol_45c76239 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "a a")?
  let r = uu.invoke(s, "tsort", ["input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
}

# origin: busybox tsort/tsort /dev/null
test test_bb_tsort_tsort_dev_null_8aa1269c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tsort", ["/dev/null"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: busybox tsort/tsort empty
test test_bb_tsort_tsort_empty_0278d08c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: busybox tsort/tsort blank
test test_bb_tsort_tsort_blank_94565657 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: b"\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: busybox tsort/tsort blanks
test test_bb_tsort_tsort_blanks_511721ef { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: b"\n\n \t\n ")?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: busybox tsort/tsort 1-edge
test test_bb_tsort_tsort_1_edge_1b03b1ac { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: b"a b\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "a\nb\n")
}

# origin: busybox tsort/tsort 2-edge
test test_bb_tsort_tsort_2_edge_e5fe24a0 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: b"a b b c\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "a\nb\nc\n")
}

# origin: busybox tsort/tsort empty2
test test_bb_tsort_tsort_empty2_a8b8e172 { |ctx|
  let s = uu.scene(ctx)?
  let words = []
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text(words.join(" ") + "\n"))?
  uu.succeeds(r)
  let output = r.stdout.utf8()?.split("\n")
  uu.no_stdout(r)
}

# origin: busybox tsort/tsort singleton
test test_bb_tsort_tsort_singleton_fc4e0c33 { |ctx|
  let s = uu.scene(ctx)?
  let words = ["a", "a"]
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text(words.join(" ") + "\n"))?
  uu.succeeds(r)
  let output = r.stdout.utf8()?.split("\n")
  for pair in range(words.len() / 2) {
    let before = [i for i in range(output.len()) if output[i] == words[pair * 2]]
    let after = [i for i in range(output.len()) if output[i] == words[pair * 2 + 1]]
    assert before.len() == 1
    assert after.len() == 1
    assert before[0] <= after[0]
  }
}

# origin: busybox tsort/tsort simple
test test_bb_tsort_tsort_simple_6c106f92 { |ctx|
  let s = uu.scene(ctx)?
  let words = ["a", "b", "b", "c"]
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text(words.join(" ") + "\n"))?
  uu.succeeds(r)
  let output = r.stdout.utf8()?.split("\n")
  for pair in range(words.len() / 2) {
    let before = [i for i in range(output.len()) if output[i] == words[pair * 2]]
    let after = [i for i in range(output.len()) if output[i] == words[pair * 2 + 1]]
    assert before.len() == 1
    assert after.len() == 1
    assert before[0] <= after[0]
  }
}

# origin: busybox tsort/tsort 2singleton
test test_bb_tsort_tsort_2singleton_7b2eeb07 { |ctx|
  let s = uu.scene(ctx)?
  let words = ["a", "a", "b", "b"]
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text(words.join(" ") + "\n"))?
  uu.succeeds(r)
  let output = r.stdout.utf8()?.split("\n")
  for pair in range(words.len() / 2) {
    let before = [i for i in range(output.len()) if output[i] == words[pair * 2]]
    let after = [i for i in range(output.len()) if output[i] == words[pair * 2 + 1]]
    assert before.len() == 1
    assert after.len() == 1
    assert before[0] <= after[0]
  }
}

# origin: busybox tsort/tsort medium
test test_bb_tsort_tsort_medium_c531ed03 { |ctx|
  let s = uu.scene(ctx)?
  let words = ["a", "b", "a", "b", "b", "c"]
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text(words.join(" ") + "\n"))?
  uu.succeeds(r)
  let output = r.stdout.utf8()?.split("\n")
  for pair in range(words.len() / 2) {
    let before = [i for i in range(output.len()) if output[i] == words[pair * 2]]
    let after = [i for i in range(output.len()) if output[i] == words[pair * 2 + 1]]
    assert before.len() == 1
    assert after.len() == 1
    assert before[0] <= after[0]
  }
}

# origin: busybox tsort/tsort std.example
test test_bb_tsort_tsort_std_example_631d5923 { |ctx|
  let s = uu.scene(ctx)?
  let words = ["a", "b", "c", "c", "d", "e", "g", "g", "f", "g", "e", "f", "h", "h"]
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text(words.join(" ") + "\n"))?
  uu.succeeds(r)
  let output = r.stdout.utf8()?.split("\n")
  for pair in range(words.len() / 2) {
    let before = [i for i in range(output.len()) if output[i] == words[pair * 2]]
    let after = [i for i in range(output.len()) if output[i] == words[pair * 2 + 1]]
    assert before.len() == 1
    assert after.len() == 1
    assert before[0] <= after[0]
  }
}

# origin: busybox tsort/tsort prefixes
test test_bb_tsort_tsort_prefixes_d11f603a { |ctx|
  let s = uu.scene(ctx)?
  let words = ["a", "aa", "aa", "aaa", "aaaa", "aaaaa", "a", "aaaaa"]
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text(words.join(" ") + "\n"))?
  uu.succeeds(r)
  let output = r.stdout.utf8()?.split("\n")
  for pair in range(words.len() / 2) {
    let before = [i for i in range(output.len()) if output[i] == words[pair * 2]]
    let after = [i for i in range(output.len()) if output[i] == words[pair * 2 + 1]]
    assert before.len() == 1
    assert after.len() == 1
    assert before[0] <= after[0]
  }
}

# origin: busybox tsort/tsort odd
test test_bb_tsort_tsort_odd_be44f3d1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: b"a\n")?
  uu.fails(r)
}

# origin: busybox tsort/tsort odd2
test test_bb_tsort_tsort_odd2_3f7dfc7c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: b"a b c\n")?
  uu.fails(r)
}

# origin: busybox tsort/tsort cycle
test test_bb_tsort_tsort_cycle_15361a16 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: b"a b b a\n")?
  uu.fails(r)
}

