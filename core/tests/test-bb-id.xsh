use support.uu

# origin: busybox id/id-g-works
test test_bb_id_id_g_works_2013668b { |ctx|
  let s = uu.scene(ctx)?
  let out = uu.at(s, "host-out")
  let err = uu.at(s, "host-err")
  let host = process.command_argv(p"/usr/bin/id", ["/usr/bin/id", "-g"], s.root, {LC_ALL: "C"}, b"", out, err, timeout: 5s)
  assert process.run(host)?.exited_with(0)
  let r = uu.invoke(s, "id", ["-g"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout == out.read_bytes()?
}

# origin: busybox id/id-u-works
test test_bb_id_id_u_works_b6470459 { |ctx|
  let s = uu.scene(ctx)?
  let out = uu.at(s, "host-out")
  let err = uu.at(s, "host-err")
  let host = process.command_argv(p"/usr/bin/id", ["/usr/bin/id", "-u"], s.root, {LC_ALL: "C"}, b"", out, err, timeout: 5s)
  assert process.run(host)?.exited_with(0)
  let r = uu.invoke(s, "id", ["-u"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout == out.read_bytes()?
}

# origin: busybox id/id-un-works
test test_bb_id_id_un_works_635aa48f { |ctx|
  let s = uu.scene(ctx)?
  let out = uu.at(s, "host-out")
  let err = uu.at(s, "host-err")
  let host = process.command_argv(p"/usr/bin/id", ["/usr/bin/id", "-un"], s.root, {LC_ALL: "C"}, b"", out, err, timeout: 5s)
  assert process.run(host)?.exited_with(0)
  let r = uu.invoke(s, "id", ["-un"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout == out.read_bytes()?
}

# origin: busybox id/id-ur-works
test test_bb_id_id_ur_works_59a8b481 { |ctx|
  let s = uu.scene(ctx)?
  let out = uu.at(s, "host-out")
  let err = uu.at(s, "host-err")
  let host = process.command_argv(p"/usr/bin/id", ["/usr/bin/id", "-ur"], s.root, {LC_ALL: "C"}, b"", out, err, timeout: 5s)
  assert process.run(host)?.exited_with(0)
  let r = uu.invoke(s, "id", ["-ur"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout == out.read_bytes()?
}

