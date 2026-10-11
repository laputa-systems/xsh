use support.uu as uu

# origin: busybox ls/ls-1-works
test test_bb_ls_ls_1_works_568f2b96 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "work")?
  uu.mkdir(s, "directory")?
  uu.write(s, "file", "listing data\n")?
  uu.touch(s, "empty")?
  uu.symlink(s, "file", "link")?
  let work = {ctx: ctx, root: uu.at(s, "work")}
  let out = uu.at(work, "capture-out")
  let err = uu.at(work, "capture-err")
  out.write(b"")?
  err.write(b"")?
  let names = ["directory", "empty", "file", "link", "work"]
  var expected = ""
  var total_blocks = 0
  for name in names {
    let entry_path = uu.at(s, name)
    if name != "link" { entry_path.chmod(if name in ["directory", "work"] { 0o755 } else { 0o644 })? }
    fs.set_times(entry_path, mtime_ns: 946771200000000000)?
    let metadata = fs.stat(entry_path)?
    total_blocks += metadata.blocks_512
    expected = f"{expected}{name}\n"
  }
  let r = uu.invoke(work, "ls", ["-1", ".."], stdout: out, stderr: err, timeout: 10s)?
  uu.succeeds(r)
  assert err.read_bytes()? == b""
  # Ignore horizontal whitespace while keeping line boundaries and every field.
  let actual = rx"[ \t\r\f\v]".replace(out.read_text()?, with: "")
  assert actual == expected, f"ls output {actual} expected {expected}"
}

# origin: busybox ls/ls-h-works
test test_bb_ls_ls_h_works_010ae205 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "work")?
  uu.mkdir(s, "directory")?
  uu.write(s, "file", "listing data\n")?
  uu.touch(s, "empty")?
  uu.symlink(s, "file", "link")?
  let work = {ctx: ctx, root: uu.at(s, "work")}
  let out = uu.at(work, "capture-out")
  let err = uu.at(work, "capture-err")
  out.write(b"")?
  err.write(b"")?
  let names = ["directory", "empty", "file", "link", "work"]
  var expected = ""
  var total_blocks = 0
  for name in names {
    let entry_path = uu.at(s, name)
    if name != "link" { entry_path.chmod(if name in ["directory", "work"] { 0o755 } else { 0o644 })? }
    fs.set_times(entry_path, mtime_ns: 946771200000000000)?
    let metadata = fs.stat(entry_path)?
    total_blocks += metadata.blocks_512
    expected = f"{expected}{name}\n"
  }
  let r = uu.invoke(work, "ls", ["-h", ".."], stdout: out, stderr: err, timeout: 10s)?
  uu.succeeds(r)
  assert err.read_bytes()? == b""
  # Ignore horizontal whitespace while keeping line boundaries and every field.
  let actual = rx"[ \t\r\f\v]".replace(out.read_text()?, with: "")
  assert actual == expected, f"ls output {actual} expected {expected}"
}

# origin: busybox ls/ls-l-works
test test_bb_ls_ls_l_works_13141269 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "work")?
  uu.mkdir(s, "directory")?
  uu.write(s, "file", "listing data\n")?
  uu.touch(s, "empty")?
  uu.symlink(s, "file", "link")?
  let work = {ctx: ctx, root: uu.at(s, "work")}
  let out = uu.at(work, "capture-out")
  let err = uu.at(work, "capture-err")
  out.write(b"")?
  err.write(b"")?
  let names = ["directory", "empty", "file", "link", "work"]
  var expected = ""
  var total_blocks = 0
  for name in names {
    let entry_path = uu.at(s, name)
    if name != "link" { entry_path.chmod(if name in ["directory", "work"] { 0o755 } else { 0o644 })? }
    fs.set_times(entry_path, mtime_ns: 946771200000000000)?
    let metadata = fs.stat(entry_path)?
    total_blocks += metadata.blocks_512
    let mode = if metadata.kind == "dir" { "drwxr-xr-x" } else if metadata.kind == "symlink" { "lrwxrwxrwx" } else { "-rw-r--r--" }
    let owner = user.by_uid(metadata.uid)?.name
    let group_name = group.by_gid(metadata.gid)?.name
    let suffix = if metadata.kind == "symlink" { f"{name}->{entry_path.readlink()?}" } else { name }
    expected = f"{expected}{mode}{metadata.nlink}{owner}{group_name}{metadata.size}Jan22000{suffix}\n"
  }
  expected = f"total{(total_blocks + 1) / 2}\n{expected}"
  let r = uu.invoke(work, "ls", ["-l", ".."], stdout: out, stderr: err, timeout: 10s)?
  uu.succeeds(r)
  assert err.read_bytes()? == b""
  # Ignore horizontal whitespace while keeping line boundaries and every field.
  let actual = rx"[ \t\r\f\v]".replace(out.read_text()?, with: "")
  assert actual == expected, f"ls output {actual} expected {expected}"
}

# origin: busybox ls/ls-s-works
test test_bb_ls_ls_s_works_ea66f4a0 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "work")?
  uu.mkdir(s, "directory")?
  uu.write(s, "file", "listing data\n")?
  uu.touch(s, "empty")?
  uu.symlink(s, "file", "link")?
  let work = {ctx: ctx, root: uu.at(s, "work")}
  let out = uu.at(work, "capture-out")
  let err = uu.at(work, "capture-err")
  out.write(b"")?
  err.write(b"")?
  let names = ["directory", "empty", "file", "link", "work"]
  var expected = ""
  var total_blocks = 0
  for name in names {
    let entry_path = uu.at(s, name)
    if name != "link" { entry_path.chmod(if name in ["directory", "work"] { 0o755 } else { 0o644 })? }
    fs.set_times(entry_path, mtime_ns: 946771200000000000)?
    let metadata = fs.stat(entry_path)?
    total_blocks += metadata.blocks_512
    expected = f"{expected}{(metadata.blocks_512 + 1) / 2}{name}\n"
  }
  expected = f"total{(total_blocks + 1) / 2}\n{expected}"
  let r = uu.invoke(work, "ls", ["-1s", ".."], stdout: out, stderr: err, timeout: 10s)?
  uu.succeeds(r)
  assert err.read_bytes()? == b""
  # Ignore horizontal whitespace while keeping line boundaries and every field.
  let actual = rx"[ \t\r\f\v]".replace(out.read_text()?, with: "")
  assert actual == expected, f"ls output {actual} expected {expected}"
}

