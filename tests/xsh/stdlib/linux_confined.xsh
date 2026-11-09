type Workspace = {declared: Path, writable: Path, outside: Path}

proc landlock_available() [process, env, fs, error] -> Result[Bool] {
  guard system.uname()?.sysname == "Linux" else {
    test.skip("confinement is Linux-only")
    return false
  }
  match linux.run_confined(process.command_argv("/bin/sh", ["sh", "-c", ":"]), read: system_paths()?) {
    Ok(_) => true
    Err(failure) => {
      guard failure.message.starts_with("confinement requires Landlock") else {
        return Err(failure)
      }
      test.skip(failure.message)
      false
    }
  }
}

proc system_paths() [fs, error] -> Result[List[Path]] {
  let found = collect {
    for dir in [fp"/bin", fp"/usr", fp"/lib", fp"/lib64"] {
      if dir.exists()? { yield dir }
    }
  }
  Ok(found)
}

proc workspace(ctx: TestContext, name: Str) [fs, error] -> Result[Workspace] {
  let root = test.temp_dir(ctx, name:)?
  let declared = fp"{root}/declared"
  let writable = fp"{root}/writable"
  let outside = fp"{root}/outside"
  for dir in [declared, writable, outside] {
    dir.mkdir()?
  }
  fp"{declared}/input".write("declared input\n")
  fp"{outside}/secret".write("outside secret\n")
  fp"{outside}/tool".write("#!/bin/sh\necho ran\n", mode: 0o755)
  Ok({declared: declared, writable: writable, outside: outside})
}

test test_linux_run_confined_allows_declared_paths_and_denies_the_rest { |ctx|
  guard landlock_available()? else { return }
  let w = workspace(ctx, "confined-fs")?
  let script = f"""
cat '{w.declared}/input' > '{w.writable}/copy' && echo read-declared ok
cat '{w.outside}/secret' 2>/dev/null && echo read-outside escaped
echo x 2>/dev/null > '{w.outside}/new' && echo write-outside escaped
echo x 2>/dev/null > '{w.declared}/new' && echo write-read-only escaped
'{w.outside}/tool' 2>/dev/null && echo exec-outside escaped
kill -0 {process.current_pid()?} 2>/dev/null && echo signal-outside escaped
echo done
"""
  let out = fp"{w.writable}/out"
  let plan = process.command_argv("/bin/sh", ["sh", "-c", script], w.writable, {}, b"", out)
  let status = linux.run_confined(plan, read: system_paths()? + [w.declared], write: [w.writable])?
  assert status.exited_with(0), out.read_text()?
  assert out.read_text()? == "read-declared ok\ndone\n"
  assert fp"{w.writable}/copy".read_text()? == "declared input\n"
  assert !fp"{w.outside}/new".exists()?
  assert !fp"{w.declared}/new".exists()?
}

proc connect_probe(
  ctx: TestContext,
  w: Workspace,
  client: Path,
  tcp: Bool,
) [fs, process, error] -> Result[Str] {
  let out = fp"{w.writable}/out-{tcp}"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin, client], w.declared, {}, b"", out)
  let status = linux.run_confined(plan, read: system_paths()? + [ctx.xsh_bin, w.declared], tcp:)?
  assert status.exited_with(0), out.read_text()?
  out.read_text()
}

test test_linux_run_confined_denies_tcp_unless_allowed { |ctx|
  guard landlock_available()? else { return }
  let w = workspace(ctx, "confined-tcp")?
  let c = linux.net_constants()
  let server = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  defer unix.close_fd(server)
  linux.bind(server, {family: "inet", address: "127.0.0.1", port: 0})
  linux.listen(server, 4)
  let port = linux.getsockname(server)?.port
  fp"{w.declared}/xsht-config.ini".write("")
  let client = fp"{w.declared}/client.xsh"
  client.write(f"""let c = linux.net_constants()
let fd = linux.socket(c.AF_INET, c.SOCK_STREAM)?
print f"no_new_privs {{linux.privileges()?.no_new_privs}}"
match linux.connect(fd, {{family: "inet", address: "127.0.0.1", port: {port}}}) {{
  Ok(_) => print "connected"
  Err(_) => print "refused"
}}
""")
  assert connect_probe(ctx, w, client, false)? == "no_new_privs true\nrefused\n"
  assert connect_probe(ctx, w, client, true)? == "no_new_privs true\nconnected\n"
}

test test_linux_run_confined_closes_inherited_descriptors { |ctx|
  guard landlock_available()? else { return }
  let w = workspace(ctx, "confined-fd")?
  var read = ""
  for dir in system_paths()? {
    read = read + f"fp\"{dir}\", "
  }
  let inner = fp"{w.declared}/inner.xsh"
  inner.write(f"""let read = [{read}]
let plain = process.command_argv("/bin/sh", ["sh", "-c", "cat <&3"], fp"{w.writable}", {{}}, b"", fp"{w.writable}/plain")
let _ = process.run(plain)?
let confined = process.command_argv("/bin/sh", ["sh", "-c", "cat <&3 2>/dev/null || echo closed"], fp"{w.writable}", {{}}, b"", fp"{w.writable}/confined")
let _ = linux.run_confined(confined, read:)?
""")
  let xsh = ctx.xsh_bin.display()
  let plan = process.command_argv("/bin/sh", ["sh", "-c", f"exec 3< '{w.outside}/secret'; exec '{xsh}' '{inner}'"], w.writable)
  assert process.run(plan)?.exited_with(0)
  assert fp"{w.writable}/plain".read_text()? == "outside secret\n", "an unconfined child inherits descriptor 3"
  assert fp"{w.writable}/confined".read_text()? == "closed\n"
}

test test_linux_run_confined_fails_before_exec_for_a_missing_declared_path { |ctx|
  guard landlock_available()? else { return }
  let w = workspace(ctx, "confined-missing")?
  let marker = fp"{w.writable}/ran"
  let plan = process.command_argv("/bin/sh", ["sh", "-c", f"touch '{marker}'"], w.writable)
  match linux.run_confined(plan, read: system_paths()? + [fp"{w.outside}/missing"], write: [w.writable]) {
    Ok(_) => assert false, "a missing declared path must fail the call"
    Err(failure) => {
      assert "cannot confine to" in failure.message, failure.message
      assert "missing" in failure.message, failure.message
    }
  }
  assert !marker.exists()?
}
