use support.uu

# origin: busybox false/false-is-silent
test test_bb_false_false_is_silent_eff3b71c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "false", [])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

# origin: busybox false/false-returns-failure
test test_bb_false_false_returns_failure_d026a992 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "false", [])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

