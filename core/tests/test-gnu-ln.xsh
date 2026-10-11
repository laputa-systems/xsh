use support.uu

proc regular_file(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  let candidate = uu.at(s, name)
  if ! candidate.exists()? { return Ok(false) }
  Ok(fs.stat(candidate, follow_symlinks: true)?.kind == "file")
}

# origin: gnu ln/backup-1.log
test test_gnu_ln_backup_1_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "b"] { uu.touch(s, name)? }
  uu.succeeds(uu.invoke(s, "ln", ["b", "b~"])?)
  uu.succeeds(uu.invoke(s, "ln", ["-f", "--b=simple", "a", "b"])?)
}

# origin: gnu ln/backup-suffix-traversal.log
test test_gnu_ln_backup_suffix_traversal_log { |ctx|
  let outer = uu.scene(ctx)?
  uu.mkdir(outer, "guard/inner")?
  let s = {ctx: outer.ctx, root: uu.at(outer, "guard/inner")}
  for name in ["a", "b"] { uu.touch(s, name)? }
  uu.mkdir(s, "subdir")?
  uu.succeeds(uu.invoke(s, "ln", ["-S", "_/../c", "-b", "-s", "a", "b"])?)
  assert ! regular_file(s, "c")?
  assert regular_file(s, "b~")?
  for name in ["b", "b~"] { uu.remove(s, name)? }
  uu.touch(s, "b")?
  uu.succeeds(uu.invoke(s, "ln", ["-b", "-s", "a", "b"], vars: {SIMPLE_BACKUP_SUFFIX: "_/../../malicious"})?)
  for name in ["malicious", "../malicious", "../../malicious"] { assert ! regular_file(s, name)? }
  assert regular_file(s, "b~")?
  for name in ["b", "b~"] { uu.remove(s, name)? }
  uu.touch(s, "b")?
  uu.succeeds(uu.invoke(s, "ln", ["-S", ".backup", "-b", "-s", "a", "b"])?)
  assert regular_file(s, "b.backup")?
  assert ! regular_file(s, "b~")?
}

# origin: gnu ln/hard-backup.log
test test_gnu_ln_hard_backup_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r = uu.invoke(s, "ln", ["--backup", "f", "f"])?
  uu.fails(r)
  uu.stderr_is(r, "ln: 'f' and 'f' are the same file\n")
}

# origin: gnu ln/hard-to-sym.log
test test_gnu_ln_hard_to_sym_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  for args in [["-L", "-s", "a", "symlink1"], ["-P", "-s", "symlink1", "symlink2"], ["-s", "-L", "-P", "symlink2", "symlink3"]] { uu.succeeds(uu.invoke(s, "ln", args)?) }
  uu.succeeds(uu.invoke(s, "ln", ["-P", "-L", "symlink3", "hard-to-a"])?)
  let logical = uu.invoke(s, "ls", ["-lG", "hard-to-a"])?
  assert logical.stdout.utf8()?.trim().ends_with("hard-to-a")
  uu.succeeds(uu.invoke(s, "ln", ["-L", "-P", "symlink3", "hard-to-3"])?)
  let physical = uu.invoke(s, "ls", ["-lG", "hard-to-3"])?
  assert physical.stdout.utf8()?.trim().ends_with("hard-to-3 -> symlink2")
  uu.succeeds(uu.invoke(s, "ln", ["-s", "/no-such-dir"])?)
  let dangling = uu.invoke(s, "ln", ["-L", "no-such-dir", "hard-to-dangle"])?
  uu.fails(dangling)
  uu.stderr_contains(dangling, " failed to access 'no-such-dir':")
  uu.succeeds(uu.invoke(s, "ln", ["-P", "no-such-dir", "hard-to-dangle"])?)
  uu.mkdir(s, "d")?
  uu.succeeds(uu.invoke(s, "ln", ["-s", "d", "link-to-dir"])?)
  for row in [{flag: "-L", name: "link-to-dir"}, {flag: "-P", name: "link-to-dir/"}] {
    let r = uu.invoke(s, "ln", [row.flag, row.name, "hard-to-dir-link"])?
    uu.fails(r)
    uu.stderr_contains(r, f": {row.name}: hard link not allowed for directory")
  }
  uu.succeeds(uu.invoke(s, "ln", ["-P", "link-to-dir", "hard-to-dir-link"])?)
}

# origin: gnu ln/non-utf8-src.log
test test_gnu_ln_non_utf8_src_log { |ctx|
  let s = uu.scene(ctx)?
  let raw = b"\xff|\xed\xba\xad|\xc2\x89|\xed\xa6\xbf\xed\xbf\xbf"
  let source = uu.at_bytes(s, raw)?
  source.write(b"a\n")?
  assert source.is_file()?
  uu.mkdir(s, "dst")?
  let name = Path.parse_bytes(raw)?
  uu.succeeds(uu.invoke_paths(s, "ln", [name, p"dst"])?)
  let dest = uu.at_bytes(s, bytes.concat([b"dst/", raw]))?
  assert dest.is_file()?
  dest.remove()?
  uu.succeeds(uu.invoke_paths(s, "ln", [p"-s", p"-t", p"dst", name])?)
  assert dest.is_symlink()?
}

# origin: gnu ln/relative.log
test test_gnu_ln_relative_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["usr/bin", "usr/lib/foo"] { uu.mkdir(s, name)? }
  uu.touch(s, "usr/lib/foo/foo")?
  let _ = uu.invoke(s, "ln", ["-sr", "usr/lib/foo/foo", "usr/bin/foo"])?
  assert uu.read_link(s, "usr/bin/foo")? == "../lib/foo/foo"
  let _ = uu.invoke(s, "ln", ["-sr", "usr/bin/foo", "usr/lib/foo/link-to-foo"])?
  assert uu.read_link(s, "usr/lib/foo/link-to-foo")? == "foo"
  let _ = uu.invoke(s, "ln", ["-s", "dir1/dir2/f", "existing_link"])?
  let _ = uu.invoke(s, "ln", ["-srf", "here", "existing_link"])?
  assert uu.read_link(s, "existing_link")? == "here"
  for row in [{target: "release1", name: "alpha"}, {target: "release2", name: "beta"}, {target: "beta", name: "latest"}] { let _ = uu.invoke(s, "ln", ["-s", row.target, row.name])? }
  uu.mkdir(s, "web")?
  let _ = uu.invoke(s, "ln", ["-sr", "latest", "web/latest"])?
  assert uu.read_link(s, "web/latest")? == "../release2"
  let empty = uu.invoke(s, "ln", ["-sr", "", "F"])?
  assert empty.status == 0 or empty.status == 1
}

# origin: gnu ln/sf-1.log
test test_gnu_ln_sf_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "foo\n")?
  uu.succeeds(uu.invoke(s, "ln", ["-s", ".", "b"])?)
  let same = uu.invoke(s, "ln", ["-sf", "a", "b"])?
  uu.fails(same)
  assert bytes.concat([same.stdout, same.stderr]).utf8()?.trim().ends_with("are the same file")
  let limit = uu.invoke(s, "stat", ["-f", "-c", "%l", "."])?
  let name_max = if limit.status == 0 { limit.stdout.utf8()?.trim().parse_int()? } else { 1 }
  let bounded = if name_max < 1048576 { name_max } else { 1 }
  let long_name = ["0" for _ in range(bounded + 1)].join("")
  for flag in ["-s", "-sf"] {
    for row in [{target: "missing", name: "ENOENT_link"}, {target: "a/b", name: "ENOTDIR_link"}, {target: "ELOOP_link", name: "ELOOP_link"}, {target: long_name, name: "ENAMETOOLONG_link"}] { uu.succeeds(uu.invoke(s, "ln", [flag, row.target, row.name])?) }
  }
}

# origin: gnu ln/slash-decorated-nonexistent-dest.log
test test_gnu_ln_slash_decorated_nonexistent_dest_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.fails_with_code(uu.invoke(s, "ln", ["-T", "f", "no-such-file/"])?, 1)
  assert ! uu.exists(s, "no-such-file")?
}

# origin: gnu ln/target-1.log
test test_gnu_ln_target_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.succeeds(uu.invoke(s, "ln", ["-s", "--target-dir=d", "../f"])?)
}

# origin: gnu ln/misc.log
test test_gnu_ln_misc_log { |ctx|
  let s = uu.scene(ctx)?
  let file = "tln-file"
  let link = "tln-symlink"
  let directory = "tln-subdir"
  let dir_link = "tln-symlink-to-subdir"
  uu.touch(s, file)?
  uu.succeeds(uu.invoke(s, "ln", ["-s", file, link])?)
  assert regular_file(s, link)?
  for name in [link, file] { uu.remove(s, name)? }
  for row in [{destination: f"{directory}/{link}", check: f"{directory}/{link}"}, {destination: directory, check: f"{directory}/{file}"}] {
    uu.touch(s, file)?
    uu.mkdir(s, directory)?
    uu.succeeds(uu.invoke(s, "ln", ["-s", f"../{file}", row.destination])?)
    assert regular_file(s, row.check)?
    for name in [directory, file] { uu.remove(s, name)? }
  }
  uu.touch(s, file)?
  uu.mkdir(s, f"{directory}/{file}")?
  for flags in [[], ["-s"]] { uu.fails_with_code(uu.invoke(s, "ln", flags.extend([file, f"{directory}/"]))?, 1) }
  for name in [directory, file] { uu.remove(s, name)? }
  for flag in ["-s", "-sf"] {
    uu.touch(s, link)?
    uu.fails_with_code(uu.invoke(s, "ln", [flag, link, link])?, 1)
    uu.remove(s, link)?
  }
  uu.mkdir(s, directory)?
  uu.touch(s, f"{directory}/{file}")?
  uu.succeeds(uu.invoke(s, "ln", ["-s", f"{directory}/{file}"])?)
  assert regular_file(s, file)?
  for name in [directory, file] { uu.remove(s, name)? }
  uu.touch(s, file)?
  uu.mkdir(s, directory)?
  let _ = uu.invoke(s, "ln", ["-s", directory, dir_link])?
  uu.succeeds(uu.invoke(s, "ln", ["-s", f"../{file}", dir_link])?)
  assert regular_file(s, f"{directory}/{file}")?
  for name in [directory, file, dir_link] { uu.remove(s, name)? }
  uu.touch(s, file)?
  uu.mkdir(s, directory)?
  let _ = uu.invoke(s, "ln", ["-s", directory, dir_link])?
  uu.succeeds(uu.invoke(s, "ln", ["--no-dereference", "-fs", uu.at(s, file).display(), dir_link])?)
  assert regular_file(s, dir_link)?
  for name in [directory, file, dir_link] { uu.remove(s, name)? }
  uu.touch(s, file)?
  uu.mkdir(s, directory)?
  let _ = uu.invoke(s, "ln", [file, directory])?
  uu.succeeds(uu.invoke(s, "ln", ["-f", file, directory])?)
  assert uu.dir_exists(s, directory)?
  for name in ["a", "b"] { uu.touch(s, name)? }
  uu.succeeds(uu.invoke(s, "ln", ["b", "b~"])?)
  uu.succeeds(uu.invoke(s, "ln", ["-f", "--b=simple", "a", "b"])?)
  for utility in ["ln", "cp", "mv", "install"] {
    for name in ["a", "x", "a.orig"] { uu.remove(s, name)? }
    for name in ["a", "x"] { uu.touch(s, name)? }
    uu.succeeds(uu.invoke(s, utility, ["--backup=simple", "--suffix=.orig", "x", "a"])?)
    assert regular_file(s, "a.orig")?
  }
  let _ = uu.invoke(s, "ln", ["foo", ""])?
  for row in [{flags: "-sif", answer: b"", symbolic: true, success: true}, {flags: "-sfi", answer: b"n\n", symbolic: false, success: false}, {flags: "-sfi", answer: b"y\n", symbolic: true, success: true}] {
    for name in ["a", "b"] { uu.remove(s, name)?; uu.touch(s, name)? }
    let r = uu.invoke(s, "ln", [row.flags, "a", "b"], stdin: row.answer)?
    if row.success { uu.succeeds(r) }
    assert uu.is_symlink(s, "b")? == row.symbolic
  }
}
