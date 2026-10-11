use support.uu as uu

proc paths(r: uu.Ran) [error] -> List[Str] {
  [line.split("\t")[1] for line in r.stdout.utf8()?.split("\n") if line != ""]
}

proc blocks(s: uu.Scene, name: Str) [fs, error] -> Int {
  fs.stat(uu.at(s, name))?.blocks_512
}

proc check_paths(s: uu.Scene, args: List[Str], expected: List[Str], ordered: Bool = true) [fs, process, env, error] {
  let r = uu.invoke(s, "du", args, timeout: 10s)?
  uu.succeeds(r)
  let actual = paths(r)
  assert (if ordered { actual } else { actual |> sort }) == expected, f"{args.join(" ")}: {actual.join(" ")}"
}

# origin: gnu du/8gb.log
test test_gnu_du_8gb_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "big")?
  uu.truncate(s, "big", 8589934592)?
  assert uu.size(s, "big")? == 8589934592
  let r = uu.invoke(s, "du", ["-ab", "big"])?
  uu.succeeds(r)
  uu.stdout_is(r, "8589934592\tbig\n")
}

# origin: gnu du/apparent.log
test test_gnu_du_apparent_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  var files: List[Str] = []
  for n in range(1, 101) {
    let name = f"d/{n}"
    uu.write(s, name, "foo\n")?
    files += [name]
  }
  files = files |> sort
  for options in [["-b"], ["-A", "-B", "1"], ["--apparent-size", "--block-size", "1"]] {
    let separate = uu.invoke(s, "du", options.extend(files))?
    let together = uu.invoke(s, "du", options.extend(["d"]))?
    uu.succeeds(separate)
    uu.succeeds(together)
    var sum = 0
    for line in separate.stdout.utf8()?.split("\n") {
      if line != "" { sum += line.split("\t")[0].parse_int()? }
    }
    assert sum == together.stdout.utf8()?.split("\t")[0].parse_int()?
  }
}

# origin: gnu du/basic.log
test test_gnu_du_basic_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.mkdir(s, "d/sub")?
  uu.write(s, "a/b/F", [" " for _ in range(226)].join("") + "make-sure-the-file-is-non-empty")?
  uu.write(s, "d/1", [" " for _ in range(4095)].join("") + "x")?
  uu.write(s, "d/sub/2", [" " for _ in range(4095)].join("") + "x")?
  uu.touch(s, "q name")?
  for style in ["literal", "shell-always", "invalid"] {
    let r = uu.invoke(s, "du", ["-b", "q name"], vars: {QUOTING_STYLE: style})?
    uu.succeeds(r)
    uu.stdout_only(r, "0\tq name\n")
  }
  let nul = uu.invoke(s, "du", ["-0b", "q name"], vars: {QUOTING_STYLE: "invalid"})?
  uu.succeeds(nul)
  uu.stdout_only_bytes(nul, b"0\tq name\0")
  let f = blocks(s, "a/b/F")
  let b = blocks(s, "a/b")
  let a = blocks(s, "a")
  for row in [
    {args: ["--block-size=512", "-a", "a"], out: f"{f}\ta/b/F\n{b + f}\ta/b\n{a + b + f}\ta\n"},
    {args: ["--block-size=512", "-a", "-S", "a"], out: f"{f}\ta/b/F\n{b + f}\ta/b\n{a}\ta\n"},
    {args: ["--block-size=512", "-s", "a"], out: f"{a + b + f}\ta\n"},
  ] {
    let r = uu.invoke(s, "du", row.args)?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
  let t2 = blocks(s, "d/sub/2")
  let ts = blocks(s, "d/sub")
  let t1 = blocks(s, "d/1")
  let td = blocks(s, "d")
  for row in [
    {args: ["--block-size=512", "-a", "d"], out: [f"{t2}\td/sub/2", f"{ts + t2}\td/sub", f"{t1}\td/1", f"{td + t1 + ts + t2}\td"]},
    {args: ["--block-size=512", "-S", "d"], out: [f"{ts + t2}\td/sub", f"{td + t1}\td"]},
  ] {
    let r = uu.invoke(s, "du", row.args)?
    uu.succeeds(r)
    assert ([line for line in r.stdout.utf8()?.split("\n") if line != ""] |> sort) == (row.out |> sort)
  }
}

# origin: gnu du/deref-args.log
test test_gnu_du_deref_args_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir/a")?
  uu.symlink(s, "dir", "slink")?
  uu.write(s, "64k", [" " for _ in range(65535)].join("") + "x")?
  uu.symlink(s, "64k", "slink-to-64k")?
  check_paths(s, ["-D", "slink"], ["slink/a", "slink"])
  check_paths(s, ["-D", "slink/"], ["slink/a", "slink/"])
  let r = uu.invoke(s, "du", ["--apparent-size", "--block-size=1K", "-D", "slink-to-64k"])?
  uu.succeeds(r)
  uu.stdout_is(r, "64\tslink-to-64k\n")
}

# origin: gnu du/deref.log
test test_gnu_du_deref_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/sub")?
  uu.symlink(s, "a/sub", "slink")?
  uu.touch(s, "b")?
  uu.symlink(s, "..", "a/sub/dotdot")?
  uu.symlink(s, "nowhere", "dangle")?
  uu.succeeds(uu.invoke(s, "du", ["-sD", "slink", "b"])?)
  uu.fails_with_code(uu.invoke(s, "du", ["-L", "dangle"])?, 1)
  let baseline = uu.invoke(s, "du", ["--exclude=dotdot", "a"])?
  uu.succeeds(baseline)
  for option in ["-L", "-lL"] {
    let r = uu.invoke(s, "du", [option, "a"])?
    uu.succeeds(r)
    assert r.stdout == baseline.stdout
  }
}

# origin: gnu du/exclude.log
test test_gnu_du_exclude_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a/b/c", "a/x/y", "a/u/v"] { uu.mkdir(s, name)? }
  uu.write(s, "excl", "b\n")?
  check_paths(s, ["--exclude=x", "a"], ["a", "a/b", "a/b/c", "a/u", "a/u/v"], false)
  check_paths(s, ["--exclude-from=excl", "a"], ["a", "a/u", "a/u/v", "a/x", "a/x/y"], false)
  let excluded = uu.invoke(s, "du", ["--exclude=a", "a"])?
  uu.succeeds(excluded)
  uu.no_stdout(excluded)
  check_paths(s, ["--exclude=a/u", "--exclude=a/b", "a"], ["a", "a/x", "a/x/y"], false)
}

# origin: gnu du/fd-leak.log
test test_gnu_du_fd_leak_log { |ctx|
  let s = uu.scene(ctx)?
  let alphabet = "abcdefghijklmnopqrstuvwxyz0123456789".split("")
  var names: List[Str] = []
  for first in alphabet { for second in alphabet {
    let name = first + second
    uu.touch(s, name)?
    names += [name]
  } }
  assert names.len() == 1296
  uu.succeeds(uu.invoke(s, "du", names, timeout: 10s)?)
}

# origin: gnu du/files0-from-dir.log
test test_gnu_du_files0_from_dir_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  for util in ["du", "wc"] {
    let r = uu.invoke(s, util, ["--files0-from=dir"])?
    uu.fails(r)
    assert r.stderr.utf8()?.split("dir:")[0] == f"{util}: "
    assert r.stderr.utf8()?.split("\n").len() == 2
  }
}

# origin: gnu du/hard-link.log
test test_gnu_du_hard_link_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir/sub")?
  uu.write(s, "dir/f1", "non-empty\n")?
  uu.hard_link(s, "dir/f1", "dir/f2")?
  uu.symlink(s, "f1", "dir/f3")?
  uu.write(s, "dir/sub/F", "non-empty\n")?
  check_paths(s, ["-a", "-L", "--exclude=sub", "--count-links", "dir"], ["dir", "dir/f1", "dir/f2", "dir/f3"], false)
  for row in [
    {args: ["-L"], out: ["dir/f_", "dir"]},
    {args: ["dir"], out: ["dir/f_", "dir/f_", "dir"]},
    {args: ["-L", "dir"], out: ["dir/f_", "dir"]},
  ] {
    let r = uu.invoke(s, "du", ["-a", "--exclude=sub"].extend(row.args).extend(["dir"]))?
    uu.succeeds(r)
    assert [name.replace("f1", with: "f_").replace("f2", with: "f_").replace("f3", with: "f_") for name in paths(r)] == row.out
  }
  uu.mkdir(s, "test-dir")?
  uu.write(s, "test-dir/file1", "content\n")?
  uu.hard_link(s, "test-dir/file1", "test-dir/file2")?
  let normal = uu.invoke(s, "du", ["test-dir"])?
  let counted = uu.invoke(s, "du", ["-l", "test-dir"])?
  uu.succeeds(normal)
  uu.succeeds(counted)
  let size = normal.stdout.utf8()?.split("\t")[0].parse_int()?
  if size > 0 { assert counted.stdout.utf8()?.split("\t")[0].parse_int()? > size }
}

# origin: gnu du/inacc-dest.log
test test_gnu_du_inacc_dest_log { |ctx|
  let scene = uu.scene(ctx)?
  uu.mkdir(scene, "f")?
  let s: uu.Scene = {ctx: ctx, root: uu.at(scene, "f")}
  for name in ["a", "b", "c", "d", "e"] { uu.mkdir(s, name)? }
  uu.touch(s, "c/j")?
  defer { uu.set_mode(s, "c", 0o700)? }
  uu.set_mode(s, "c", 0o666)?
  let r = uu.invoke(s, "du", [])?
  uu.fails(r)
  let diagnostic = r.stderr.utf8()?.replace("/c/j':", with: "/c':").replace("cannot access", with: "cannot read directory")
  let actual = paths(r).extend([line for line in diagnostic.split("\n") if line != ""]) |> sort
  assert actual == [".", "./a", "./b", "./c", "./d", "./e", "du: cannot read directory './c': Permission denied"]
}

# origin: gnu du/inacc-dir.log
test test_gnu_du_inacc_dir_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/sub")?
  defer { uu.set_mode(s, "a/sub", 0o700)? }
  for option in ["-s", "-k"] {
    uu.set_mode(s, "a/sub", 0o700)?
    let baseline = uu.invoke(s, "du", [option, "a"])?
    uu.succeeds(baseline)
    uu.set_mode(s, "a/sub", 0)?
    let failed = uu.invoke(s, "du", [option, "a"])?
    uu.fails(failed)
    assert failed.stdout == baseline.stdout
  }
}

# The wrapper changes permissions only after entering the directory, preserving
# a cwd that can no longer be opened by pathname by the applet.
proc unreadable_cwd(s: uu.Scene, operands: List[Path]) [fs, process, env, error] {
  uu.mkdir(s, "inaccessible")?
  let out = uu.at(s, "wrapped-out")
  let err = uu.at(s, "wrapped-err")
  let code = r"""denied=$1; shift; cd "$denied" || exit 99; chmod 000 "$denied" || exit 99; "$@"; result=$?; chmod 700 "$denied"; exit "$result"; """
  let words = [p"/bin/sh", p"-c", Path(code), p"du-cwd", uu.at(s, "inaccessible")].extend(uu.argv(s, "du", operands)?)
  let result = process.run(process.command_argv(p"/bin/sh", words, s.root, stdout: out, stderr: err, timeout: 20s))?
  assert result.exited_with(0), err.read_text()?
}

# origin: gnu du/inaccessible-cwd.log
test test_gnu_du_inaccessible_cwd_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  unreadable_cwd(s, [uu.at(s, "a")])
}

# origin: gnu du/long-from-unreadable.log
test test_gnu_du_long_from_unreadable_log { |ctx|
  let s = uu.scene(ctx)?
  let component = ["x" for _ in range(200)].join("")
  var directory = fs.open_root(s.root)?
  for _ in range(52) {
    directory.mkdir(Path(component))?
    let child = directory.open_root(Path(component))?
    directory.close()
    directory = child
  }
  directory.close()
  unreadable_cwd(s, [p"-s", uu.at(s, component)])
}

# origin: gnu du/max-depth.log
test test_gnu_du_max_depth_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b/c/d/e")?
  check_paths(s, ["--max-depth=2", "a"], ["a/b/c", "a/b", "a"])
  check_paths(s, ["-d", "1", "a"], ["a/b", "a"])
  let invalid = uu.invoke(s, "du", ["-d", "-1", "a"])?
  uu.fails_with_code(invalid, 1)
  uu.stderr_only(invalid, "du: invalid maximum depth '-1'\nTry 'du --help' for more information.\n")
}

# origin: gnu du/no-deref.log
test test_gnu_du_no_deref_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir/a/b")?
  uu.symlink(s, "dir", "slink")?
  check_paths(s, ["slink"], ["slink"])
}

# origin: gnu du/no-x.log
test test_gnu_du_no_x_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d/no-x/y")?
  defer { uu.set_mode(s, "d/no-x", 0o700)? }
  uu.set_mode(s, "d/no-x", 0o600)?
  let r = uu.invoke(s, "du", ["d"])?
  uu.fails(r)
  assert r.stderr.utf8()?.replace("cannot access ", with: "").replace("cannot read directory ", with: "").replace("d/no-x/y", with: "d/no-x") == "du: 'd/no-x': Permission denied\n"
}

# origin: gnu du/one-file-system.log
test test_gnu_du_one_file_system_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "b/c")?
  uu.mkdir(s, "y/z")?
  uu.mkdir(s, "d")?
  let elsewhere = run.capture --text mktemp -d /dev/shm/xsh-du-XXXXXXXX
  assert elsewhere.status.exited_with(0), elsewhere.stderr
  let other = Path(elsewhere.stdout.trim())
  defer { other.remove()? }
  fp"{other}/x".mkdir()?
  assert fs.stat(other)?.dev != fs.stat(s.root)?.dev
  uu.symlink(s, fp"{other}/x".display(), "d/x")?
  check_paths(s, ["-ax", "b", "y"], ["b/c", "b", "y/z", "y"])
  check_paths(s, ["-xL", "d"], ["d"])
  uu.touch(s, "f")?
  for option in ["-x", "-xs"] { check_paths(s, [option, "f"], ["f"]) }
}

# origin: gnu du/restore-wd.log
test test_gnu_du_restore_wd_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "b")?
  uu.succeeds(uu.invoke(s, "du", ["a", "b"])?)
}

# origin: gnu du/slash.log
test test_gnu_du_slash_log { |ctx|
  let s = uu.scene(ctx)?
  check_paths(s, ["--exclude=[^/]*", "-x", "/"], ["/"])
}

# origin: gnu du/trailing-slash.log
test test_gnu_du_trailing_slash_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir/1/2")?
  uu.symlink(s, "dir", "slink")?
  check_paths(s, ["slink/"], ["slink/1/2", "slink/1", "slink/"])
  check_paths(s, ["-L", "slink"], ["slink/1/2", "slink/1", "slink"])
}

# origin: gnu du/two-args.log
test test_gnu_du_two_args_log { |ctx|
  let parent = uu.scene(ctx)?
  uu.mkdir(parent, "sub/t/1")?
  uu.mkdir(parent, "sub/t/2")?
  let s: uu.Scene = {ctx: ctx, root: uu.at(parent, "sub")}
  assert uu.dir_exists(s, "t")?
  for operands in [["t/1", "t/2"], [".", "t"], ["..", "t"]] {
    uu.succeeds(uu.invoke(s, "du", operands, timeout: 10s)?)
  }
}

# origin: gnu du/inodes.log
test test_gnu_du_inodes_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  let empty = uu.invoke(s, "du", ["--inodes", "d"])?
  uu.succeeds(empty)
  uu.stdout_only(empty, "1\td\n")
  uu.touch(s, "d/f")?
  let file = uu.invoke(s, "du", ["--inodes", "d"])?
  uu.succeeds(file)
  uu.stdout_only(file, "2\td\n")
  uu.hard_link(s, "d/f", "d/h")?
  for row in [
    {args: ["--inodes", "d"], out: "2\td\n"},
    {args: ["--inodes", "-l", "d"], out: "3\td\n"},
  ] {
    let r = uu.invoke(s, "du", row.args)?
    uu.succeeds(r)
    uu.stdout_only(r, row.out)
  }
  uu.mkdir(s, "d/d")?
  for row in [
    {args: ["--inodes", "-s", "d"], out: "3\td\n"},
    {args: ["--inodes", "-S", "d"], out: "1\td/d\n2\td\n"},
    {args: ["--inodes", "d"], out: "1\td/d\n3\td\n"},
    {args: ["--inodes", "-c", "d"], out: "1\td/d\n3\td\n3\ttotal\n"},
  ] {
    let r = uu.invoke(s, "du", row.args)?
    uu.succeeds(r)
    uu.stdout_only(r, row.out)
  }
  let all = uu.invoke(s, "du", ["--inodes", "-a", "d"])?
  uu.succeeds(all)
  uu.no_stderr(all)
  assert ([line.replace("h", with: "f") for line in all.stdout.utf8()?.split("\n") if line != ""] |> sort) == ["1\td/d", "1\td/f", "3\td"]
  let links = uu.invoke(s, "du", ["--inodes", "-al", "d"])?
  uu.succeeds(links)
  uu.no_stderr(links)
  assert ([line for line in links.stdout.utf8()?.split("\n") if line != ""] |> sort) == ["1\td/d", "1\td/f", "1\td/h", "4\td"]
  uu.touch(s, "d/d/f")?
  let nested = uu.invoke(s, "du", ["--inodes", "d"])?
  uu.succeeds(nested)
  uu.stdout_only(nested, "2\td/d\n4\td\n")
  uu.remove(s, "d")?
  uu.mkdir(s, "d")?
  for n in range(1, 1024) { uu.touch(s, f"d/file{n}")? }
  for row in [
    {args: ["--inodes", "-h", "d"], out: "1.0K\td\n"},
    {args: ["--inodes", "--si", "d"], out: "1.1k\td\n"},
    {args: ["--inodes", "-B10", "d"], out: "1024\td\n"},
    {args: ["--inodes", "--threshold=1000", "d"], out: "1024\td\n"},
    {args: ["--inodes", "--threshold=-1000", "d"], out: ""},
  ] {
    let r = uu.invoke(s, "du", row.args)?
    uu.succeeds(r)
    uu.stdout_only(r, row.out)
  }
  for option in ["-b", "--apparent-size"] {
    let r = uu.invoke(s, "du", ["--inodes", option, "d"])?
    uu.succeeds(r)
    uu.stderr_contains(r, " ineffective ")
  }
  let help = uu.invoke(s, "du", ["--help"])?
  uu.succeeds(help)
  uu.stdout_contains(help, "--inodes")
}

# origin: gnu du/2g.log
test test_gnu_du_2g_log { |ctx|
  let made = run.capture --text mktemp -d /var/tmp/xsh-du-XXXXXXXX
  assert made.status.exited_with(0), made.stderr
  let allocated: uu.Scene = {ctx: ctx, root: Path(made.stdout.trim())}
  defer { allocated.root.remove()? }
  let big = uu.at(allocated, "big")
  let allocation = run.capture --text fallocate -l2G $big
  assert allocation.status.exited_with(0), allocation.stderr
  let flushed = run.capture --text sync $big
  assert flushed.status.exited_with(0), flushed.stderr
  let r = uu.invoke(allocated, "du", ["-k", "big"])?
  uu.succeeds(r)
  let line = r.stdout.utf8()?
  assert line.ends_with("\tbig\n")
  let kb = line.split("\t")[0]
  assert rx"^2[0-9]{6}$".matches(kb), line
}
