use support.uu

# origin: busybox uptime/uptime-works
test test_bb_uptime_uptime_works_ca97fe56 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uptime", [], timeout: 5s)?
  uu.succeeds(r)
}
