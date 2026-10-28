type Fixture = {root: FsRoot, dir: Path}

const ELOOP = 40
const EISDIR = 21
const EEXIST = 17
const EBUSY = 16

proc failure_errno(result: Result[Unit]) -> Int {
  match result {
    Ok(_) => -1
    Err(error) => error.errno ?? -2
  }
}

proc open_fixture(ctx: TestContext) [fs, process, error] -> Result[Fixture] {
  let dir = test.temp_dir(ctx, name: "write-atomic")?
  {root: fs.open_root(dir)?, dir: dir}
}

test test_write_atomic_defaults_replace_a_symlink_itself { |ctx|
  let fixture = open_fixture(ctx)?
  fp"{fixture.dir}/target".write("target")
  fp"{fixture.dir}/link".symlink(to: p"target")
  fixture.root.write_atomic(p"link", "replaced")
  assert fp"{fixture.dir}/target".read_text()? == "target"
  assert fp"{fixture.dir}/link".read_text()? == "replaced"
  assert fixture.root.stat(p"link")?.kind == "file"
}

test test_write_atomic_mode_is_exact { |ctx|
  let fixture = open_fixture(ctx)?
  fixture.root.write_atomic(p"private", b"secret", mode: 0o600)
  fixture.root.write_atomic(p"public", "open", mode: 0o644)
  assert fixture.root.stat(p"private")?.mode.bit_and(0o7777) == 0o600
  assert fixture.root.stat(p"public")?.mode.bit_and(0o7777) == 0o644
  match fixture.root.write_atomic(p"bad", "x", mode: 0o10000) {
    Ok(_) => assert false, "a mode over 0o7777 must fail"
    Err(error) => assert "mode" in error.message
  }
  assert ! fixture.root.exists(p"bad")?
}

test test_write_atomic_regular_or_absent_names_what_it_refuses { |ctx|
  let fixture = open_fixture(ctx)?
  fixture.root.write_atomic(p"file", "one", replace: "regular_or_absent")
  fixture.root.write_atomic(p"file", "two", mode: 0o640, replace: "regular_or_absent")
  assert fixture.root.read_text(p"file")? == "two"

  fp"{fixture.dir}/outside".write("outside")
  fp"{fixture.dir}/link".symlink(to: p"outside")
  fixture.root.mkdir(p"dir")
  assert failure_errno(fixture.root.write_atomic(p"link", "new", replace: "regular_or_absent")) == ELOOP
  assert failure_errno(fixture.root.write_atomic(p"dir", "new", replace: "regular_or_absent")) == EISDIR
  assert fp"{fixture.dir}/outside".read_text()? == "outside"
  assert fixture.root.stat(p"link")?.kind == "symlink"
  assert fixture.root.stat(p"dir")?.kind == "dir"
  for entry in fs.children(fixture.dir)? {
    assert ".tmp." not in entry.name, "no temporary survives a refused replacement"
  }
}

test test_write_atomic_rejects_an_unknown_replace_policy { |ctx|
  let fixture = open_fixture(ctx)?
  match fixture.root.write_atomic(p"file", "x", replace: "always") {
    Ok(_) => assert false, "an unknown policy must fail"
    Err(error) => assert "replace" in error.message
  }
  assert ! fixture.root.exists(p"file")?
}

test test_write_atomic_onto_a_bind_mounted_file_fails_ebusy_and_keeps_the_mount { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("bind mounts are Linux-only")
    return
  }
  let dir = test.temp_dir(ctx, name: "write-atomic-mount")?
  let script = test.temp_file(
    ctx,
    name: "bind.xsh",
    contents: bytes.from_text(
      """proc main(...argv: List[Str]) {
  let dir = fp"{argv[0]}"
  fp"{dir}/a".write("a")
  fp"{dir}/b".write("b")
  linux.mount(bytes.from_text(f"{dir}/b"), fp"{dir}/a", options: ["bind"])?
  let root = fs.open_root(dir)?
  let before = fp"{dir}/a".read_text()?
  let failed = match root.write_atomic(p"a", "replaced") {
    Ok(_) => -1
    Err(error) => error.errno ?? -2
  }
  print f"{failed} {before} {fp"{dir}/a".read_text()?} {fp"{dir}/b".read_text()?}"
}
""",
    ),
  )?
  let out = fp"{dir}/out"
  let err = fp"{dir}/err"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin, script, dir], dir, {}, b"", out, err, timeout: 60s)
  match linux.run_in_namespaces(plan, unshare: ["user", "mnt"], map_root_user: true) {
    Ok(status) => {
      guard status.shell_code()? == 0 else {
        test.skip(f"the kernel refuses a private mount namespace: {err.read_text()?.trim()}")
        return
      }
      assert out.read_text()?.trim() == f"{EBUSY} b b b"
    }
    Err(failure) => test.skip(f"the kernel refuses user and mount namespaces: {failure.message}")
  }
}
