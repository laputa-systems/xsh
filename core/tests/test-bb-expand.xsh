use support.uu as uu

# origin: busybox expand/expand
test test_bb_expand_expand_80a37da4 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", [], stdin: bytes.from_text("\t12345678\t12345678\n"))?
  uu.succeeds(r)
  uu.stdout_only(r, "        12345678        12345678\n")
}

# origin: busybox expand/expand with unicode characher 0x394
test test_bb_expand_expand_with_unicode_characher_0x394_1aded4f4 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", [], stdin: bytes.from_text("Δ\t12345ΔΔΔ\t12345678\n"))?
  uu.succeeds(r)
  uu.stdout_only(r, "Δ      12345ΔΔΔ     12345678\n")
}

