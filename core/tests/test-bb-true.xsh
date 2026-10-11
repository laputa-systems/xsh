use support.uu

# origin: busybox true/true-is-silent
test test_bb_true_true_is_silent_e6da1e57 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "true", [])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: busybox true/true-returns-success
test test_bb_true_true_returns_success_97432a79 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "true", [])?
  uu.succeeds(r)
  uu.no_output(r)
}

