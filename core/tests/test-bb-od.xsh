use support.uu as uu

# origin: busybox od/od -b
test test_bb_od_od_b_11110155 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "od", ["-b"], stdin: b"HELLO")?
  uu.succeeds(r)
  uu.stdout_only(r, "0000000 110 105 114 114 117\n0000005\n")
}

# origin: busybox od/od -b --traditional
test test_bb_od_od_b_traditional_4b4de318 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "od", ["-b", "--traditional"], stdin: b"HELLO")?
  uu.succeeds(r)
  uu.stdout_only(r, "0000000 110 105 114 114 117\n0000005\n")
}

# origin: busybox od/od -b --traditional FILE
test test_bb_od_od_b_traditional_FILE_bf8d848f { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "HELLO")?
  let r = uu.invoke(s, "od", ["-b", "--traditional", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "0000000 110 105 114 114 117\n0000005\n")
}

