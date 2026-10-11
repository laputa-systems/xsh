use support.uu as uu

# origin: busybox sed/sed NUL in command
test test_bb_sed_sed_NUL_in_command_badc0c39 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "sed.commands", b"/woo/a he\0llo\n")?
  let r = uu.invoke(s, "sed", ["-f", "sed.commands"], stdin: b"woo", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"woo\nhe\0llo\n")
}

# origin: busybox sed/sed embedded NUL
test test_bb_sed_sed_embedded_NUL_79e7c43c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/woo/bang/"], stdin: b"\0woo\0woo\0", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"\0bang\0woo\0")
}

# origin: busybox sed/sed nonexistent label
test test_bb_sed_sed_nonexistent_label_d5082b36 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "b walrus"], stdin: b"", timeout: 5s)?
  uu.fails(r)
  uu.stdout_is_bytes(r, b"")
}
