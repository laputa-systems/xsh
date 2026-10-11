use support.uu as uu

type DiskUsage = {blocks: Int, rows: Str, seen: List[Str]}

# Sum allocated kernel blocks once per device/inode, without following links.
proc disk_usage(entry_path: Path, seen: List[Str]) [fs, error] -> Result[DiskUsage] {
  let metadata = fs.stat(entry_path)?
  let identity = f"{metadata.dev}:{metadata.ino}"
  if identity in seen { return Ok({blocks: 0, rows: "", seen: seen}) }
  var identities = seen.push(identity)
  var blocks = metadata.blocks_512
  var rows = ""
  if metadata.kind == "dir" {
    for child in fs.children(entry_path)? {
      let usage = disk_usage(child.path, identities)?
      blocks += usage.blocks
      rows = f"{rows}{usage.rows}"
      identities = usage.seen
    }
    rows = f"{rows}{(blocks + 1) / 2}\t{entry_path}\n"
  }
  Ok({blocks: blocks, rows: rows, seen: identities})
}

# origin: busybox du/du-h-works
test test_bb_du_du_h_works_c6df7262 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "file", bytes.concat([b"\0" for _ in range(1048576)]))?
  let r = uu.invoke(s, "du", ["-h", "file"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1.0M\tfile\n")
}

# origin: busybox du/du-k-works
test test_bb_du_du_k_works_97253bd7 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "du.testdir")?
  let directory = {ctx: ctx, root: uu.at(s, "du.testdir")}
  uu.write_bytes(directory, "file1", bytes.concat([b"\0" for _ in range(65536)]))?
  uu.write_bytes(directory, "file2", bytes.concat([b"\0" for _ in range(16384)]))?
  let r = uu.invoke(directory, "du", ["-k", "."])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout in [b"80\t.\n", b"81\t.\n", b"82\t.\n", b"83\t.\n", b"84\t.\n", b"88\t.\n"]
}

# origin: busybox du/du-l-works
test test_bb_du_du_l_works_bf1ec30b { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "du.testdir")?
  let directory = {ctx: ctx, root: uu.at(s, "du.testdir")}
  uu.write_bytes(directory, "file1", bytes.concat([b"\0" for _ in range(65536)]))?
  uu.write_bytes(directory, "file2", bytes.concat([b"\0" for _ in range(16384)]))?
  uu.hard_link(directory, "file1", "file1.1")?
  let r = uu.invoke(directory, "du", ["-l", "."])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout in [b"144\t.\n", b"146\t.\n", b"148\t.\n", b"152\t.\n", b"156\t.\n"]
}

# origin: busybox du/du-m-works
test test_bb_du_du_m_works_3eda7af5 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "file", bytes.concat([b"\0" for _ in range(1048576)]))?
  let r = uu.invoke(s, "du", ["-m", "file"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\tfile\n")
}

# origin: busybox du/du-s-works
test test_bb_du_du_s_works_d2c1b741 { |ctx|
  let s = uu.scene(ctx)?
  let usage = disk_usage(p"/bin", [])?
  let r = uu.invoke(s, "du", ["-s", "/bin"], timeout: 10s)?
  uu.succeeds(r)
  uu.no_stderr(r)
  let expected = f"{(usage.blocks + 1) / 2}\t/bin\n"
  uu.stdout_is(r, expected)
}

# origin: busybox du/du-works
test test_bb_du_du_works_1ac86a3e { |ctx|
  let s = uu.scene(ctx)?
  let usage = disk_usage(p"/bin", [])?
  let r = uu.invoke(s, "du", ["/bin"], timeout: 10s)?
  uu.succeeds(r)
  uu.no_stderr(r)
  let expected = if fs.stat(p"/bin")?.kind == "dir" { usage.rows } else { f"{(usage.blocks + 1) / 2}\t/bin\n" }
  uu.stdout_is(r, expected)
}

