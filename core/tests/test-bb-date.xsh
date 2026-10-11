use support.uu

# origin: busybox date/date-@-works
test test_bb_date_date_works_4a64c371 { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "date", ["-d", "@1288486799"], vars: {TZ: "EET-2EEST,M3.5.0/3,M10.5.0/4"})?
    uu.succeeds(r)
    uu.stdout_only(r, "Sun Oct 31 03:59:59 EEST 2010\n")
  }
  {
    let r = uu.invoke(s, "date", ["-d", "@1288486801"], vars: {TZ: "EET-2EEST,M3.5.0/3,M10.5.0/4"})?
    uu.succeeds(r)
    uu.stdout_only(r, "Sun Oct 31 03:00:01 EET 2010\n")
  }
  {
    let r = uu.invoke(s, "date", ["-d", "@1269737999"], vars: {TZ: "EET-2EEST,M3.5.0/3,M10.5.0/4"})?
    uu.succeeds(r)
    uu.stdout_only(r, "Sun Mar 28 02:59:59 EET 2010\n")
  }
  {
    let r = uu.invoke(s, "date", ["-d", "@1269738001"], vars: {TZ: "EET-2EEST,M3.5.0/3,M10.5.0/4"})?
    uu.succeeds(r)
    uu.stdout_only(r, "Sun Mar 28 04:00:01 EEST 2010\n")
  }
}

# origin: busybox date/date-R-works
test test_bb_date_date_R_works_7b91452c { |ctx|
  let s = uu.scene(ctx)?
  var matched = false
  for attempt in range(8) {
    let out = uu.at(s, "host-out")
    let err = uu.at(s, "host-err")
    let host = process.command_argv(p"/bin/date", [p"/bin/date", p"-R"], s.root, {LC_ALL: "C", TZ: "UTC"}, b"", out, err, timeout: 5s)
    assert process.run(host)?.exited_with(0)
    let expected = out.read_bytes()?
    let r = uu.invoke(s, "date", ["-R"])?
    uu.succeeds(r)
    uu.no_stderr(r)
    if r.stdout == expected { matched = true; break }
  }
  assert matched, "RFC date must match the host date within eight attempts"
}

# origin: busybox date/date-timezone
test test_bb_date_date_timezone_876fad85 { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "date", ["-d", "1999-1-2 3:4:5Z"], vars: {TZ: "UTC0"})?
    uu.succeeds(r)
    uu.no_stderr(r)
    assert r.stdout.slice(0, 19) == b"Sat Jan  2 03:04:05"
  }
  {
    let r = uu.invoke(s, "date", ["-d", "1999-1-2 3:4:5 +0600"], vars: {TZ: "UTC0"})?
    uu.succeeds(r)
    uu.no_stderr(r)
    assert r.stdout.slice(0, 19) == b"Fri Jan  1 21:04:05"
  }
  {
    let r = uu.invoke(s, "date", ["-d", "1999-1-2 3:4:5 -0600"], vars: {TZ: "UTC0"})?
    uu.succeeds(r)
    uu.no_stderr(r)
    assert r.stdout.slice(0, 19) == b"Sat Jan  2 09:04:05"
  }
  {
    let r = uu.invoke(s, "date", ["-d", "2021-03-28 00:59:59 +0000"], vars: {TZ: "GMT0BST,M3.5.0/1,M10.5.0/2"})?
    uu.succeeds(r)
    uu.stdout_only(r, "Sun Mar 28 00:59:59 GMT 2021\n")
  }
  {
    let r = uu.invoke(s, "date", ["-d", "2021-03-28 01:00:01 +0000"], vars: {TZ: "GMT0BST,M3.5.0/1,M10.5.0/2"})?
    uu.succeeds(r)
    uu.stdout_only(r, "Sun Mar 28 02:00:01 BST 2021\n")
  }
  {
    let r = uu.invoke(s, "date", ["-d", "2021-10-31 00:00:01 +0000"], vars: {TZ: "GMT0BST,M3.5.0/1,M10.5.0/2"})?
    uu.succeeds(r)
    uu.stdout_only(r, "Sun Oct 31 01:00:01 BST 2021\n")
  }
  {
    let r = uu.invoke(s, "date", ["-d", "2021-10-31 01:00:01 +0000"], vars: {TZ: "GMT0BST,M3.5.0/1,M10.5.0/2"})?
    uu.succeeds(r)
    uu.stdout_only(r, "Sun Oct 31 01:00:01 GMT 2021\n")
  }
}
