use support.uu as uu

# origin: busybox basename/basename-does-not-remove-identical-extension
test test_bb_basename_basename_does_not_remove_identical_extension_ece99a16 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["foo", "foo"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo\n")
}

# origin: busybox basename/basename-works
test test_bb_basename_basename_works_bcaaa21c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", [s.root.display()])?
  uu.succeeds(r)
  uu.stdout_only(r, f"{s.root.name()}\n")
}
