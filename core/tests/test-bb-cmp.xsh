use support.uu as uu

# origin: busybox cmp/cmp-detects-difference
test test_bb_cmp_cmp_detects_difference_5755da9f { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "foo\n")?
  uu.write(s, "bar", "bar\n")?
  let r = uu.invoke(s, "cmp", ["-s", "foo", "bar"], timeout: 5s)?
  uu.fails(r)
}
