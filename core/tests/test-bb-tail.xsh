use support.uu as uu

# origin: busybox tail/tail-n-works
test test_bb_tail_tail_n_works_e2be15cb { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "abc\ndef\n123\n")?
  let r = uu.invoke(s, "tail", ["-n", "2", "input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "def\n123\n")
}

# origin: busybox tail/tail-works
test test_bb_tail_tail_works_18477a36 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "abc\ndef\n123\n")?
  let r = uu.invoke(s, "tail", ["-2", "input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "def\n123\n")
}

# origin: busybox tail/tail: +N with N > file length
test test_bb_tail_tail_N_with_N_file_length_0e46d620 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-c", "+55"], stdin: b"qw")?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: busybox tail/tail: -c +N with largish N
test test_bb_tail_tail_c_N_with_largish_N_ea39c957 { |ctx|
  let s = uu.scene(ctx)?
  let input = bytes.concat([b"\0" for _ in range(16384)])
  for row in [{offset: "8200", size: 8185}, {offset: "8208", size: 8177}] {
    let r = uu.invoke(s, "tail", ["-c", f"+{row.offset}"], stdin: input)?
    uu.succeeds(r)
    uu.no_stderr(r)
    assert r.stdout.len() == row.size
  }
}

