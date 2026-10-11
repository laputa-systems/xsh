use support.uu

# origin: busybox hostid/hostid-works
test test_bb_hostid_hostid_works_50d836d1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "hostid", [])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let value = rx"\n+$".replace(r.stdout.utf8()?, with: "")
  assert rx"^[0-9a-f]*$".matches(value), value
}
