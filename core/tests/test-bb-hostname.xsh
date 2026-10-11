use support.uu

# origin: busybox hostname/hostname-d-works
test test_bb_hostname_hostname_d_works_e9855ad2 { |ctx|
  let s = uu.scene(ctx)?
  let full = uu.invoke(s, "hostname", ["-f"])?
  let domain = uu.invoke(s, "hostname", ["-d"])?
  uu.succeeds(full)
  uu.succeeds(domain)
  uu.no_stderr(full)
  uu.no_stderr(domain)
  let fqdn = rx"\n+$".replace(full.stdout.utf8()?, with: "") + "."
  let suffix = rx"^[^.]*\.".replace(fqdn, with: "")
  let value = rx"\n+$".replace(domain.stdout.utf8()?, with: "")
  assert suffix == value + (if value == "" { "" } else { "." })
}

# origin: busybox hostname/hostname-i-works
test test_bb_hostname_hostname_i_works_89c47685 { |ctx|
  let s = uu.scene(ctx)?
  let out = uu.at(s, "host-out")
  let err = uu.at(s, "host-err")
  let host = process.command_argv(p"/bin/hostname", ["/bin/hostname", "-i"], s.root, {LC_ALL: "C"}, b"", out, err, timeout: 5s)
  assert process.run(host)?.exited_with(0)
  let r = uu.invoke(s, "hostname", ["-i"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout == out.read_bytes()?
}

# origin: busybox hostname/hostname-s-works
test test_bb_hostname_hostname_s_works_a1c00700 { |ctx|
  let s = uu.scene(ctx)?
  let out = uu.at(s, "host-out")
  let err = uu.at(s, "host-err")
  let host = process.command_argv(p"/bin/hostname", ["/bin/hostname", "-s"], s.root, {LC_ALL: "C"}, b"", out, err, timeout: 5s)
  assert process.run(host)?.exited_with(0)
  let r = uu.invoke(s, "hostname", ["-s"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout == out.read_bytes()?
}

# origin: busybox hostname/hostname-works
test test_bb_hostname_hostname_works_d83b936c { |ctx|
  let s = uu.scene(ctx)?
  let out = uu.at(s, "host-out")
  let err = uu.at(s, "host-err")
  let host = process.command_argv(p"/bin/hostname", ["/bin/hostname"], s.root, {LC_ALL: "C"}, b"", out, err, timeout: 5s)
  assert process.run(host)?.exited_with(0)
  let r = uu.invoke(s, "hostname", [])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout == out.read_bytes()?
}

