use support.uu

# A second tmpfs forces copying rather than rename; cleanup belongs to each test.
proc other_fs(s: uu.Scene) [fs, error] -> Result[Path, Error] {
  let dir = fp"/dev/shm/mv-{s.root.basename()}"
  dir.mkdir()?
  dir.chmod(0o700)?
  assert fs.stat(dir)?.dev != fs.stat(s.root)?.dev, "move test requires distinct devices"
  Ok(dir)
}

proc traced_move(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let argv = [p"strace", p"-qe", p"trace=unlink"].extend(uu.argv(s, "mv", [Path(arg) for arg in args])?)
  let out = uu.at(s, ".trace-out")
  let err = uu.at(s, ".trace-err")
  let status = process.run(process.command_argv(p"strace", argv, s.root, {}, b"", out, err, timeout: 10s))?
  Ok({util: "mv", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

proc linked(left: Path, right: Path) [fs, error] -> Bool {
  let a = fs.stat(left)?
  let b = fs.stat(right)?
  a.dev == b.dev and a.ino == b.ino
}

# origin: gnu mv/atomic.log
test test_gnu_mv_atomic_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "t1", "s1")?
  uu.symlink(s, "t2", "s2")?
  let r = traced_move(s, ["-T", "s1", "s2"])?
  uu.succeeds(r)
  assert ! regex.compile("unlink.*\"s1\"")?.matches(r.stderr.utf8()?)
  assert ! uu.exists(s, "s1")?
  assert uu.read_link(s, "s2")? == "t1"
}

# origin: gnu mv/atomic2.log
test test_gnu_mv_atomic2_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  uu.hard_link(s, "b", "b2")?
  let r = traced_move(s, ["a", "b"])?
  uu.succeeds(r)
  assert ! regex.compile("unlink.*\"b\"")?.matches(r.stderr.utf8()?)
  assert ! uu.exists(s, "a")?
  assert fs.stat(uu.at(s, "b"))?.nlink == 1
}

# origin: gnu mv/backup-dir.log
test test_gnu_mv_backup_dir_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["A", "B"] { uu.mkdir(s, name)? }
  for name in ["X", "Y"] { uu.touch(s, name)? }
  let r = uu.invoke(s, "mv", ["--verbose", "--backup=numbered", "-T", "A", "B"])?
  uu.succeeds(r)
  uu.stdout_is(r, "renamed 'A' -> 'B' (backup: 'B.~1~')\n")
  for name in ["C", "D", "E"] { uu.mkdir(s, name)? }
  uu.succeeds(uu.invoke(s, "mv", ["-T", "--backup=numbered", "C", "E/"])?)
  uu.succeeds(uu.invoke(s, "mv", ["-T", "--backup=numbered", "D", "E/"])?)
  uu.mkdir(s, "F")?
  uu.write(s, "1", "1\n")?
  uu.write(s, "2", "2\n")?
  uu.write(s, "F/X", "1\n")?
  uu.write(s, "X", "2\n")?
  uu.succeeds(uu.invoke(s, "mv", ["--backup=simple", "X", "F/"])?)
  uu.file_is(s, "F/X~", "1\n")
  uu.file_is(s, "F/X", "2\n")
}

# origin: gnu mv/backup-is-src.log
test test_gnu_mv_backup_is_src_log { |ctx|
  let s = uu.scene(ctx)?
  let remote = other_fs(s)?
  defer remote.remove()
  fp"{remote}/a".write("a\n")?
  fp"{remote}/a~".write("a2\n")?
  let r = uu.invoke(s, "mv", ["--b=simple", f"{remote}/a~", f"{remote}/a"])?
  uu.fails(r)
  assert bytes.concat([r.stdout, r.stderr]).utf8()? == f"mv: backing up '{remote}/a' might destroy source;  '{remote}/a~' not moved\n"
}

# origin: gnu mv/childproof.log
test test_gnu_mv_childproof_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "b", "c"] { uu.mkdir(s, name)? }
  uu.write(s, "a/f", "a\n")?
  uu.write(s, "b/f", "b\n")?
  uu.fails_with_code(uu.invoke(s, "cp", ["a/f", "b/f", "c"])?, 1)
  for name in ["a/f", "b/f", "c/f"] { assert uu.file_exists(s, name)? }
  uu.file_is(s, "c/f", "a\n")
  uu.remove(s, "c/f")?
  uu.succeeds(uu.invoke(s, "cp", ["--backup=numbered", "a/f", "b/f", "c"])?)
  for name in ["a/f", "b/f", "c/f", "c/f.~1~"] { assert uu.file_exists(s, name)? }
  uu.remove(s, "c/f")?
  uu.remove(s, "c/f.~1~")?
  uu.fails_with_code(uu.invoke(s, "mv", ["a/f", "b/f", "c"])?, 1)
  assert ! uu.exists(s, "a/f")?
  assert uu.file_exists(s, "b/f")?
  uu.file_is(s, "c/f", "a\n")
  uu.remove(s, "c/f")?
  uu.remove(s, "b/f")?
  uu.touch(s, "a/f")?
  uu.hard_link(s, "a/f", "b/g")?
  uu.succeeds(uu.invoke(s, "mv", ["a/f", "b/g", "c"])?)
  for name in ["a/f", "b/g"] { assert ! uu.exists(s, name)? }
  for name in ["c/f", "c/g"] { assert uu.file_exists(s, name)? }
  for name in ["a/f", "b/f", "b/g"] { uu.touch(s, name)? }
  uu.fails_with_code(uu.invoke(s, "mv", ["a/f", "b/f", "b/g", "c"])?, 1)
  for name in ["a/f", "b/g"] { assert ! uu.exists(s, name)? }
  for name in ["b/f", "c/f", "c/g"] { assert uu.file_exists(s, name)? }
  for name in ["a/f", "b/f", "c/f"] { uu.remove(s, name)? }
  uu.write(s, "a/f", "a\n")?
  uu.write(s, "b/f", "b\n")?
  uu.fails_with_code(uu.invoke(s, "ln", ["-f", "a/f", "b/f", "c"])?, 1)
  assert linked(uu.at(s, "a/f"), uu.at(s, "c/f"))
  assert ! linked(uu.at(s, "b/f"), uu.at(s, "c/f"))
}

# origin: gnu mv/dir-file.log
test test_gnu_mv_dir_file_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir/file")?
  uu.touch(s, "file")?
  uu.fails_with_code(uu.invoke(s, "mv", ["dir", "file"])?, 1)
  uu.fails_with_code(uu.invoke(s, "mv", ["file", "dir"])?, 1)
}

# origin: gnu mv/dir2dir.log
test test_gnu_mv_dir2dir_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a/t", "b/t"] { uu.mkdir(s, name)? }
  uu.touch(s, "a/t/f")?
  let r = uu.invoke(s, "mv", ["b/t", "a"])?
  uu.fails(r)
  uu.stderr_is(r, "mv: cannot overwrite 'a/t': Directory not empty\n")
}

# origin: gnu mv/force.log
test test_gnu_mv_force_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "mvforce", "force-contents\n")?
  uu.hard_link(s, "mvforce", "mvforce2")?
  for target in ["mvforce", "mvforce2"] {
    let r = uu.invoke(s, "mv", ["mvforce", target])?
    uu.fails(r)
    assert bytes.concat([r.stdout, r.stderr]).utf8()? == f"mv: 'mvforce' and '{target}' are the same file\n"
    uu.file_is(s, "mvforce", "force-contents\n")
  }
}

# origin: gnu mv/hard-2.log
test test_gnu_mv_hard_2_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dst")?
  for name in ["a", "b", "c"] { uu.touch(s, f"dst/{name}")? }
  uu.touch(s, "a")?
  for name in ["b", "c"] { uu.hard_link(s, "a", name)? }
  uu.succeeds(uu.invoke(s, "cp", ["--preserve=link", "a", "b", "c", "dst"])?)
  for name in ["a", "b", "c"] { assert uu.file_exists(s, name)?; assert uu.file_exists(s, f"dst/{name}")? }
  assert linked(uu.at(s, "dst/a"), uu.at(s, "dst/b"))
  assert linked(uu.at(s, "dst/a"), uu.at(s, "dst/c"))
  for name in ["a", "b", "c"] { uu.remove(s, f"dst/{name}")?; uu.touch(s, f"dst/{name}")? }
  uu.succeeds(uu.invoke(s, "mv", ["a", "b", "c", "dst"])?)
  for name in ["a", "b", "c"] { assert ! uu.exists(s, name)?; assert uu.file_exists(s, f"dst/{name}")? }
  assert linked(uu.at(s, "dst/a"), uu.at(s, "dst/b"))
  assert linked(uu.at(s, "dst/a"), uu.at(s, "dst/c"))
}

# origin: gnu mv/hard-3.log
test test_gnu_mv_hard_3_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["x", "dst/x"] { uu.mkdir(s, name)? }
  uu.touch(s, "dst/x/b")?
  uu.set_mode(s, "dst/x", 0o555)?
  defer uu.set_mode(s, "dst/x", 0o755)
  uu.touch(s, "a")?
  for name in ["x/b", "c"] { uu.hard_link(s, "a", name)? }
  uu.fails(uu.invoke(s, "cp", ["--preserve=link", "--parents", "a", "x/b", "c", "dst"])?)
  for name in ["a", "x/b", "c"] { assert uu.file_exists(s, name)?; assert uu.file_exists(s, f"dst/{name}")? }
  assert linked(uu.at(s, "dst/a"), uu.at(s, "dst/c"))
  assert ! linked(uu.at(s, "dst/a"), uu.at(s, "dst/x/b"))
}

# origin: gnu mv/hard-4.log
test test_gnu_mv_hard_4_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.hard_link(s, "a", "b")?
  let r = uu.invoke(s, "mv", ["a", "b"])?
  uu.fails(r)
  uu.stderr_is(r, "mv: 'a' and 'b' are the same file\n")
  for name in ["a", "b"] { assert uu.file_exists(s, name)? }
  uu.succeeds(uu.invoke(s, "mv", ["--backup=simple", "a", "b"])?)
  assert ! uu.exists(s, "a")?
  for name in ["b", "b~"] { assert uu.file_exists(s, name)? }
}

# origin: gnu mv/hard-link-1.log
test test_gnu_mv_hard_link_1_log { |ctx|
  let s = uu.scene(ctx)?
  let remote = other_fs(s)?
  defer remote.remove()
  uu.mkdir(s, "hlink")?
  uu.touch(s, "hlink/a")?
  uu.hard_link(s, "hlink/a", "hlink/b")?
  uu.succeeds(uu.invoke(s, "mv", ["hlink", remote.display()])?)
  assert linked(fp"{remote}/hlink/a", fp"{remote}/hlink/b")
}

# origin: gnu mv/i-1.log
test test_gnu_mv_i_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "src", "a\n")?
  uu.write(s, "dst", "b\n")?
  let r = uu.invoke(s, "mv", ["-i", "src", "dst"], stdin: b"n\n")?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "")
  uu.stderr_is(r, "mv: overwrite 'dst'? ")
  assert uu.file_exists(s, "src")?
}

# origin: gnu mv/i-2.log
test test_gnu_mv_i_2_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "b", "c", "d", "e", "f", "g", "h"] { uu.write(s, name, f"{name}\n")? }
  for name in ["b", "d", "f", "h"] { uu.set_mode(s, name, 0)? }
  uu.write(s, "y", "y\n")?
  uu.succeeds(uu.invoke(s, "mv", ["-if", "a", "b"])?)
  uu.succeeds(uu.invoke(s, "mv", ["-fi", "c", "d"], stdin: b"y\n")?)
  let r = uu.invoke(s, "cp", ["-if", "e", "f"], stdin: b"y\n")?
  uu.succeeds(r)
  assert bytes.concat([r.stdout, r.stderr]).utf8()?.starts_with("cp: replace 'f', overriding mode 0000 (---------)?")
  for name in ["e", "f"] { assert uu.file_exists(s, name)? }
  assert uu.read(s, "e")? == uu.read(s, "f")?
  uu.succeeds(uu.invoke(s, "cp", ["-fi", "g", "h"], stdin: b"y\n")?)
  for name in ["g", "h"] { assert uu.file_exists(s, name)? }
  assert uu.read(s, "g")? == uu.read(s, "h")?
}

# origin: gnu mv/i-4.log
test test_gnu_mv_i_4_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "b", "y", "n"] { uu.write(s, name, f"{name}\n")? }
  uu.succeeds(uu.invoke(s, "mv", ["-i", "a", "b"], stdin: b"y\n")?)
  uu.file_is(s, "b", "a\n")
  uu.remove(s, "b")?
  uu.write(s, "a", "a\n")?
  uu.hard_link(s, "a", "b")?
  let r = uu.invoke(s, "mv", ["-i", "a", "b"], stdin: b"y\n")?
  uu.fails(r)
  for name in ["a", "b"] { assert uu.file_exists(s, name)? }
  uu.stderr_is(r, "mv: 'a' and 'b' are the same file\n")
}

# origin: gnu mv/i-5.log
test test_gnu_mv_i_5_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.touch(s, "b")?
  uu.fails_with_code(uu.invoke(s, "mv", ["-i", "a", "b"], stdin: b"n\n")?, 1)
}

# origin: gnu mv/i-link-no.log
test test_gnu_mv_i_link_no_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "b"] { uu.mkdir(s, name)? }
  uu.write(s, "a/foo", "foo\n")?
  uu.hard_link(s, "a/foo", "a/bar")?
  uu.write(s, "b/FUBAR", "FUBAR\n")?
  uu.hard_link(s, "b/FUBAR", "b/bar")?
  uu.set_mode(s, "b/bar", 0o444)?
  uu.write(s, "no", "n\n")?
  let r = uu.invoke(s, "mv", ["a/bar", "a/foo", "b"], stdin: b"n\n")?
  uu.succeeds(r)
  uu.no_output(r)
  uu.file_is(s, "b/foo", "foo\n")
}

# origin: gnu mv/into-self-2.log
test test_gnu_mv_into_self_2_log { |ctx|
  let s = uu.scene(ctx)?
  let remote = other_fs(s)?
  defer remote.remove()
  let file = fp"{remote}/file"
  file.write("whatever\n")?
  uu.symlink(s, file.display(), "symlink")?
  let r = uu.invoke(s, "mv", ["symlink", file.display()])?
  uu.fails(r)
  assert bytes.concat([r.stdout, r.stderr]).utf8()? == f"mv: 'symlink' and '{file}' are the same file\n"
  uu.succeeds(uu.invoke(s, "mv", [file.display(), "symlink"])?)
}

# origin: gnu mv/into-self-3.log
test test_gnu_mv_into_self_3_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["is3-dir1", "is3-dir2"] { uu.mkdir(s, name)? }
  let r = uu.invoke(s, "mv", ["is3-dir1", "is3-dir2", "is3-dir2"])?
  uu.fails(r)
  assert bytes.concat([r.stdout, r.stderr]).utf8()? == "mv: cannot move 'is3-dir2' to a subdirectory of itself, 'is3-dir2/is3-dir2'\n"
}

# origin: gnu mv/into-self-4.log
test test_gnu_mv_into_self_4_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.symlink(s, "file", "s")?
  uu.fails_with_code(uu.invoke(s, "mv", ["s", "s"])?, 1)
  assert fs.stat(uu.at(s, "s"), follow_symlinks: true)?.kind == "file"
}

# origin: gnu mv/into-self.log
test test_gnu_mv_into_self_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "toself-dir/a/b")?
  uu.touch(s, "toself-file")?
  let r = uu.invoke(s, "mv", ["toself-dir", "toself-file", "toself-dir"])?
  uu.fails(r)
  assert bytes.concat([r.stdout, r.stderr]).utf8()? == "mv: cannot move 'toself-dir' to a subdirectory of itself, 'toself-dir/toself-dir'\n"
  assert ! uu.exists(s, "toself-file")?
  assert uu.dir_exists(s, "toself-dir")?
  assert ! uu.exists(s, "toself-dir/toself-dir")?
  assert uu.file_exists(s, "toself-dir/toself-file")?
}

# origin: gnu mv/leak-fd.log
test test_gnu_mv_leak_fd_log { |ctx|
  let s = uu.scene(ctx)?
  let remote = other_fs(s)?
  defer remote.remove()
  let letters = "a b c d e f g h i j k l m n o p q r s t u v w x y z".split(" ")
  let alphabet = [f"{i}" for i in range(10)].extend(letters).extend([f"_{letter.upper()}" for letter in letters])
  var names: List[Str] = []
  for prefix in alphabet {
    names = names.extend([prefix]).extend([f"{prefix}{suffix}" for suffix in alphabet])
  }
  uu.write(s, ".dirs", f"{names.join("\n")}\n")?
  for name in names { uu.mkdir(s, name)?; uu.touch(s, f"{name}/f")? }
  assert uu.file_exists(s, f"{names[names.len() - 1]}/f")?
  uu.succeeds(uu.invoke(s, "mv", (names |> sort).extend([remote.display()]), timeout: 300s)?)
  assert ! uu.exists(s, f"{names[names.len() - 1]}/f/f")?
  uu.remove(s, ".dirs")?
  assert (fs.files(s.root, hidden: true) |> count()) == 0
}

# origin: gnu mv/mv-exchange.log
test test_gnu_mv_mv_exchange_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.mkdir(s, "b")?
  let r = uu.invoke(s, "mv", ["-T", "--exchange", "a", "b"])?
  if r.status == 0 {
    assert uu.dir_exists(s, "a")?
    assert uu.file_exists(s, "b")?
  } else {
    uu.stderr_contains(r, "Operation not supported")
  }
  uu.touch(s, "c")?
  for args in [["--exchange", "a"], ["--exchange", "a", "b", "c"], ["--exchange", "a", "d"]] {
    uu.fails_with_code(uu.invoke(s, "mv", args)?, 1)
  }
}

# origin: gnu mv/mv-n.log
test test_gnu_mv_mv_n_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [
    {options: "-vi", answer: b"n\n", code: 1, output: ""},
    {options: "-vi", answer: b"y\n", code: 0, output: "renamed 'a' -> 'b'\n"},
    {options: "-vin", answer: b"y\n", code: 0, output: ""},
    {options: "-in", answer: b"y\n", code: 0, output: ""},
    {options: "-vfn", answer: b"y\n", code: 0, output: ""},
    {options: "-vifn", answer: b"y\n", code: 0, output: ""},
  ] {
    for name in ["a", "b"] { uu.touch(s, name)? }
    let r = uu.invoke(s, "mv", [row.options, "a", "b"], stdin: row.answer)?
    uu.fails_with_code(r, row.code)
    uu.stdout_is(r, row.output)
    if row.options == "-in" { uu.no_stderr(r) }
  }
  uu.touch(s, "a")?
  for options in [["-bn"], ["-b", "--update=none"], ["-b", "--update=none-fail"]] {
    uu.fails_with_code(uu.invoke(s, "mv", options.extend(["a", "b"]))?, 1)
  }
}

# origin: gnu mv/mv-special-1.log
test test_gnu_mv_mv_special_1_log { |ctx|
  let s = uu.scene(ctx)?
  let remote = other_fs(s)?
  defer remote.remove()
  uu.mkfifo(s, "mv-null")?
  for name in ["mv-dir/a/b/c", "mv-dir/d/e/f", "mv-dir2"] { uu.mkdir(s, name)? }
  for name in ["mv-dir/a/b/c/file1", "mv-dir/d/e/f/file2"] { uu.touch(s, name)? }
  uu.mkfifo(s, "mv-dir2/mv-null")?
  assert fs.stat(uu.at(s, "mv-null"))?.kind == "fifo"
  assert fs.stat(uu.at(s, "mv-dir2/mv-null"))?.kind == "fifo"
  let r = uu.invoke(s, "mv", ["-v", "mv-null", "mv-dir", "mv-dir2", remote.display()], timeout: 60s)?
  uu.succeeds(r)
  for name in ["mv-null", "mv-dir", "mv-dir2/mv-null"] { assert ! uu.exists(s, name)? }
  assert fs.stat(fp"{remote}/mv-null")?.kind == "fifo"
  assert fp"{remote}/mv-dir/a/b/c".is_dir()?
  assert fs.stat(fp"{remote}/mv-dir2/mv-null")?.kind == "fifo"
  var actual: List[Str] = []
  for line in r.stdout.utf8()?.lines() {
    if line.starts_with("removed ") { continue }
    let normalized = line.replace("renamed ", with: "").replace("copied ", with: "").replace(remote.display(), with: "XXX")
    if normalized.starts_with("created directory 'XXX/") {
      let name = normalized[23..normalized.byte_len() - 1]
      actual = actual.extend([f"'{name}' -> 'XXX/{name}'"])
    } else { actual = actual.extend([normalized]) }
  }
  let entries = ["mv-null", "mv-dir", "mv-dir/a", "mv-dir/a/b", "mv-dir/a/b/c", "mv-dir/a/b/c/file1", "mv-dir/d", "mv-dir/d/e", "mv-dir/d/e/f", "mv-dir/d/e/f/file2", "mv-dir2", "mv-dir2/mv-null"]
  assert (actual |> sort) == ([f"'{name}' -> 'XXX/{name}'" for name in entries] |> sort)
  # Socket nodes need no live server and exercise replacement of a regular file.
  fs.mknod(uu.at(s, "mv-sock"), "socket", 0o600)?
  fs.mknod(fp"{remote}/test.sock", "socket", 0o600)?
  fp"{remote}/mv-sock-dest".write("")?
  uu.succeeds(uu.invoke(s, "mv", ["mv-sock", f"{remote}/mv-sock-dest"])?)
  assert fs.stat(fp"{remote}/mv-sock-dest")?.kind == "socket"
  assert ! uu.exists(s, "mv-sock")?
}

# origin: gnu mv/no-copy.log
test test_gnu_mv_no_copy_log { |ctx|
  let s = uu.scene(ctx)?
  let remote = other_fs(s)?
  defer remote.remove()
  uu.mkdir(s, "dir")?
  for name in ["dir/a", "file"] { uu.touch(s, name)? }
  for name in ["dir", "file"] { uu.fails_with_code(uu.invoke(s, "mv", ["--no-copy", name, remote.display()])?, 1) }
  for name in ["dir", "file"] { uu.succeeds(uu.invoke(s, "mv", [name, remote.display()])?) }
}

# origin: gnu mv/no-target-dir.log
test test_gnu_mv_no_target_dir_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["d/sub", "empty", "src", "d2/sub", "e2", "a", "b/a"] { uu.mkdir(s, name)? }
  uu.touch(s, "f")?
  uu.succeeds(uu.invoke(s, "mv", ["a", "b"])?)
  uu.succeeds(uu.invoke(s, "mv", ["-fT", "d", "empty"])?)
  uu.fails_with_code(uu.invoke(s, "ls", ["-d", "d"])?, 2)
  assert uu.dir_exists(s, "empty/sub")?
  uu.fails_with_code(uu.invoke(s, "mv", ["-fT", "src", "d2"])?, 1)
  uu.fails_with_code(uu.invoke(s, "mv", ["-fT", "f", "e2"])?, 1)
}

# origin: gnu mv/part-fail.log
test test_gnu_mv_part_fail_log { |ctx|
  let s = uu.scene(ctx)?
  let remote = other_fs(s)?
  defer remote.remove()
  uu.touch(s, "k")?
  fp"{remote}/k".write("")?
  remote.chmod(0o500)?
  defer remote.chmod(0o700)
  let r = uu.invoke(s, "mv", ["-f", "k", remote.display()])?
  uu.fails(r)
  let actual = r.stderr.utf8()?
  assert actual == f"mv: inter-device move failed: 'k' to '{remote}/k'; unable to remove target: Permission denied\n" or actual == f"mv: cannot move 'k' to '{remote}/k': Permission denied\n"
}

# origin: gnu mv/part-hardlink-symlink.log
test test_gnu_mv_part_hardlink_symlink_log { |ctx|
  let s = uu.scene(ctx)?
  let remote = other_fs(s)?
  defer remote.remove()
  uu.mkdir(s, "d")?
  uu.write(s, "d/realfile", "important data")?
  uu.hard_link(s, "d/realfile", "d/realfile2")?
  uu.symlink(s, "realfile", "d/link1")?
  uu.succeeds(uu.invoke(s, "mv", ["d", remote.display()])?)
  assert ! uu.exists(s, "d")?
  let dest = fp"{remote}/d"
  for name in ["realfile", "realfile2"] {
    assert fp"{dest}/{name}".is_file()?
    assert ! fp"{dest}/{name}".is_symlink()?
    assert fp"{dest}/{name}".read_bytes()? == b"important data"
  }
  assert linked(fp"{dest}/realfile", fp"{dest}/realfile2")
  assert fp"{dest}/link1".is_symlink()?
  assert fp"{dest}/link1".readlink()? == p"realfile"
  for name in ["a", "b"] { fp"{dest}/{name}".mkdir()? }
  uu.mkdir(s, "c")?
  for name in ["symlink1", "symlink2"] { fp"{dest}/a/{name}".symlink(to: p".")? }
  let nested = {ctx: s.ctx, root: uu.at(s, "c")}
  let r = uu.invoke(nested, "mv", [f"{dest}/a", f"{dest}/b/"], timeout: 10s)?
  uu.succeeds(r)
  uu.no_output(r)
  assert ! fp"{dest}/a".exists()?
  assert fp"{dest}/b".is_dir()?
  assert fp"{dest}/b/a".is_dir()?
  for name in ["symlink1", "symlink2"] {
    assert fp"{dest}/b/a/{name}".is_symlink()?
    assert fp"{dest}/b/a/{name}".readlink()? == p"."
  }
}

# origin: gnu mv/part-hardlink.log
test test_gnu_mv_part_hardlink_log { |ctx|
  let s = uu.scene(ctx)?
  let remote = other_fs(s)?
  defer remote.remove()
  uu.touch(s, "f")?
  uu.hard_link(s, "f", "g")?
  for name in ["a", "b"] { uu.mkdir(s, name)? }
  uu.touch(s, "a/1")?
  uu.hard_link(s, "a/1", "b/1")?
  uu.succeeds(uu.invoke(s, "mv", ["f", "g", remote.display()])?)
  uu.succeeds(uu.invoke(s, "mv", ["a", "b", remote.display()])?)
  assert linked(fp"{remote}/f", fp"{remote}/g")
  assert linked(fp"{remote}/a/1", fp"{remote}/b/1")
}

# origin: gnu mv/part-rename.log
test test_gnu_mv_part_rename_log { |ctx|
  let s = uu.scene(ctx)?
  let remote = other_fs(s)?
  defer remote.remove()
  uu.mkdir(s, "foo")?
  uu.succeeds(uu.invoke(s, "mv", ["foo/", f"{remote}/bar"])?)
  uu.touch(s, "bar")?
  uu.fails_with_code(uu.invoke(s, "mv", ["bar", f"{remote}/"])?, 1)
  uu.mkdir(s, "bar2")?
  fp"{remote}/bar2".write("")?
  uu.fails_with_code(uu.invoke(s, "mv", ["bar2", f"{remote}/"])?, 1)
  uu.mkdir(s, "bar3")?
  uu.touch(s, "bar3/file")?
  fp"{remote}/bar3".mkdir()?
  uu.succeeds(uu.invoke(s, "mv", ["bar3", f"{remote}/"])?)
  assert fp"{remote}/bar3/file".exists()?
  uu.mkdir(s, "bar3")?
  uu.touch(s, "bar3/file2")?
  uu.fails_with_code(uu.invoke(s, "mv", ["bar3", f"{remote}/"])?, 1)
  assert ! fp"{remote}/bar3/file2".exists()?
}

# origin: gnu mv/partition-perm.log
test test_gnu_mv_partition_perm_log { |ctx|
  let s = uu.scene(ctx)?
  let remote = other_fs(s)?
  defer remote.remove()
  uu.touch(s, "file")?
  uu.set_mode(s, "file", 0o777)?
  uu.succeeds(uu.invoke(s, "mv", ["file", remote.display()], umask: 0o077)?)
  assert ! uu.exists(s, "file")?
  assert fp"{remote}/file".is_file()?
  assert fs.stat(fp"{remote}/file")?.mode % 4096 == 0o777
}

# origin: gnu mv/perm-1.log
test test_gnu_mv_perm_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "no-write/dir")?
  uu.set_mode(s, "no-write", 0o555)?
  defer uu.set_mode(s, "no-write", 0o755)
  let r = uu.invoke(s, "mv", ["no-write/dir", "."])?
  uu.fails(r)
  assert bytes.concat([r.stdout, r.stderr]).utf8()? == "mv: cannot move 'no-write/dir' to './dir': Permission denied\n"
}

# origin: gnu mv/symlink-onto-hardlink-to-self.log
test test_gnu_mv_symlink_onto_hardlink_to_self_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.symlink(s, "f", "s2")?
  uu.hard_link(s, "s2", "s1")?
  assert uu.is_symlink(s, "s1")?
  let r = uu.invoke(s, "mv", ["s1", "s2"])?
  uu.fails(r)
  uu.stderr_is(r, "mv: 's1' and 's2' are the same file\n")
  assert uu.exists(s, "s1")?
  assert ! uu.exists(s, "s2~")?
  let backup = uu.invoke(s, "mv", ["--backup", "s1", "s2"])?
  uu.succeeds(backup)
  uu.no_output(backup)
  assert ! uu.exists(s, "s1")?
  assert uu.read_link(s, "s2~")? == "f"
}

# origin: gnu mv/symlink-onto-hardlink.log
test test_gnu_mv_symlink_onto_hardlink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.hard_link(s, "f", "h")?
  uu.symlink(s, "f", "s")?
  let r = uu.invoke(s, "mv", ["s", "f"])?
  uu.fails(r)
  uu.stderr_only(r, "mv: 's' and 'f' are the same file\n")
  let moved = uu.invoke(s, "mv", ["s", "l"])?
  uu.succeeds(moved)
  uu.no_output(moved)
}

# origin: gnu mv/to-symlink.log
test test_gnu_mv_to_symlink_log { |ctx|
  let s = uu.scene(ctx)?
  let remote = other_fs(s)?
  defer remote.remove()
  uu.write(s, "to-sym", "local\n")?
  fp"{remote}/file".write("remote\n")?
  fp"{remote}/symlink".symlink(to: fp"{remote}/file")?
  uu.succeeds(uu.invoke(s, "mv", ["to-sym", f"{remote}/symlink"])?)
  assert ! uu.exists(s, "to-sym")?
  assert fp"{remote}/file".read_text()? == "remote\n"
}

# origin: gnu mv/trailing-slash.log
test test_gnu_mv_trailing_slash_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "foo")?
  uu.succeeds(uu.invoke(s, "mv", ["foo/", "bar"])?)
  for util in ["mv", "cp"] {
    for option in [[], ["-T"], ["-u"]] {
      for name in ["d", "e"] { uu.remove(s, name)? }
      uu.mkdir(s, "d")?
      let base = if util == "cp" { ["-r"] } else { [] }
      uu.succeeds(uu.invoke(s, util, base.extend(option).extend(["d", "e/"]))?)
      assert uu.exists(s, "d")? == (util == "cp")
      assert uu.dir_exists(s, "e")?
    }
  }
  uu.touch(s, "b")?
  let r = uu.invoke(s, "cp", ["b", "no-such/"])?
  assert r.stderr.utf8()?.replace("No such file or directory", with: "Not a directory") == "cp: cannot create regular file 'no-such/': Not a directory\n"
}

proc update_files(s: uu.Scene) [fs, time, error] -> Result[Unit, Error] {
  uu.write(s, "old", "old\n")?
  let yesterday = (time.now() - 86400000) * 1000000
  fs.set_times(uu.at(s, "old"), atime_ns: yesterday, mtime_ns: yesterday)?
  uu.write(s, "new", "new\n")?
  Ok()
}

# origin: gnu mv/update.log
test test_gnu_mv_update_log { |ctx|
  let s = uu.scene(ctx)?
  update_files(s)?
  for interactive in [[], ["-i"]] {
    for util in ["cp", "mv"] {
      let r = uu.invoke(s, util, interactive.extend(["--update", "old", "new"]))?
      uu.succeeds(r)
      uu.no_output(r)
      uu.file_is(s, "new", "new\n")
      uu.file_is(s, "old", "old\n")
    }
  }
  uu.fails_with_code(uu.invoke(s, "mv", ["-vi", "-u", "new", "old"], stdin: b"n\n")?, 1)
  for option in ["--update", "--update=older", "--update=all", "--update=none", "--update=none-fail"] {
    uu.touch(s, "file1")?
    uu.succeeds(uu.invoke(s, "mv", [option, "file1", "file2"])?)
    assert ! uu.exists(s, "file1")?
    uu.succeeds(uu.invoke(s, "cp", [option, "file2", "file1"])?)
    for name in ["file1", "file2"] { uu.remove(s, name)? }
  }
  for options in [["--update"], ["--update=older"], ["--update=all"], ["--update=none", "--update=all"]] {
    for util in ["mv", "cp"] {
      update_files(s)?
      uu.succeeds(uu.invoke(s, util, options.extend(["new", "old"]))?)
      uu.file_is(s, "old", "new\n")
      if util == "mv" { assert ! uu.exists(s, "new")? } else { uu.file_is(s, "new", "new\n") }
    }
  }
  for options in [["--update=none"], ["--update=none-fail"], ["--update=all", "--update=none"], ["--update=all", "--no-clobber"], ["--no-clobber", "--update=all"]] {
    for util in ["mv", "cp"] {
      update_files(s)?
      let r = uu.invoke(s, util, options.extend(["new", "old"]))?
      uu.fails_with_code(r, if "--update=none-fail" in options { 1 } else { 0 })
      uu.file_is(s, "new", "new\n")
      uu.file_is(s, "old", "old\n")
    }
  }
}
