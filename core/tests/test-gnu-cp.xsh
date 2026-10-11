use support.uu

proc copied(s: uu.Scene, args: List[Str], mask: Int? = null) [fs, process, env, error] {
  uu.succeeds(uu.invoke(s, "cp", args, umask: mask)?)
}

proc inode(s: uu.Scene, name: Str) [fs, error] -> Result[Int, Error] {
  Ok(fs.stat(uu.at(s, name))?.ino)
}

proc host(s: uu.Scene, args: List[Str]) [fs, process, error] -> Result[Bytes, Error] {
  let out = uu.at(s, ".host-out")
  let err = uu.at(s, ".host-err")
  let status = process.run(process.command_argv(args[0], args, s.root, {LC_ALL: "C", TZ: "UTC"}, b"", out, err, timeout: 10s))?
  assert status.shell_code()? == 0, f"fixture command {args.join(" ")}: {err.read_text()?}"
  Ok(out.read_bytes()?)
}

# Wrappers establish descriptor and synchronization boundaries while retaining
# the oracle's applet launcher and argument transport.
proc wrapped(s: uu.Scene, args: List[Str], body: Str, mask: Int? = null) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let launch = uu.argv(s, "cp", [Path(arg) for arg in args], umask: mask)?
  let words = [p"/bin/sh", p"-c", Path(body), p"cp-wrapper"].extend(launch)
  let out = uu.at(s, ".wrapper-out")
  let err = uu.at(s, ".wrapper-err")
  let status = process.run(process.command_argv(p"/bin/sh", words, s.root, {LC_ALL: "C", TZ: "UTC"}, b"", out, err, timeout: 65s))?
  Ok({util: "cp", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# origin: gnu cp/abuse.log
test test_gnu_cp_abuse_log { |ctx|
  let s = uu.scene(ctx)?
  for dir in ["a", "b", "c"] { uu.mkdir(s, dir)? }
  uu.symlink(s, "../t", "a/1")?
  uu.write(s, "b/1", "payload\n")?
  for existing in [false, true] {
    if existing { uu.write(s, "t", "i\n")? }
    let r = uu.invoke(s, "cp", ["-dR", "a/1", "b/1", "c"])?
    uu.fails(r)
    uu.stderr_is(r, "cp: will not copy 'b/1' through just-created symlink 'c/1'\n")
    if existing { uu.file_is(s, "t", "i\n") } else { assert !uu.exists(s, "t")? }
  }
}

# origin: gnu cp/attr-existing.log
test test_gnu_cp_attr_existing_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "1")?
  uu.write(s, "file2", "2")?
  copied(s, ["--attributes-only", "file1", "file2"])
  uu.file_is(s, "file2", "2")
  uu.hard_link(s, "file2", "link2")?
  copied(s, ["-a", "--attributes-only", "file1", "file2"])
  uu.file_is(s, "file2", "2")
  uu.symlink(s, "file1", "sym1")?
  uu.fails_with_code(uu.invoke(s, "cp", ["-a", "--attributes-only", "sym1", "file2"])?, 1)
  uu.file_is(s, "file2", "2")
  copied(s, ["-a", "--remove-destination", "--attributes-only", "sym1", "file2"])
  assert uu.is_symlink(s, "file2")?
  uu.file_is(s, "file2", "1")
}

# origin: gnu cp/backup-1.log
test test_gnu_cp_backup_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "F", "test\n")?
  copied(s, ["--force", "--backup=simple", "--suffix=.b", "F", "F"])
  copied(s, ["-T", "--force", "--backup=simple", "--suffix=.b", "F", "F"])
  assert uu.file_exists(s, "F")? and uu.file_exists(s, "F.b")?
  assert uu.read(s, "F")? == uu.read(s, "F.b")?
}

# origin: gnu cp/backup-dir.log
test test_gnu_cp_backup_dir_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "x")?
  uu.mkdir(s, "y")?
  copied(s, ["-a", "x", "y"])
  copied(s, ["-ab", "x", "y"])
  assert uu.dir_exists(s, "y/x")? and !uu.exists(s, "y/x~")?
  for dir in ["src/foo", "dst/foo"] { uu.mkdir(s, dir)?; uu.touch(s, f"{dir}/bar")? }
  copied(s, ["--recursive", "--backup", "src/foo", "dst"])
}

# origin: gnu cp/backup-is-src.log
test test_gnu_cp_backup_is_src_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\n")?
  uu.write(s, "a~", "a-tilde\n")?
  let r = uu.invoke(s, "cp", ["--b=simple", "a~", "a"])?
  uu.fails(r)
  uu.stderr_only(r, "cp: backing up 'a' might destroy source;  'a~' not copied\n")
}

# origin: gnu cp/cp-HL.log
test test_gnu_cp_cp_HL_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "src-dir")?
  uu.mkdir(s, "dest-dir")?
  uu.write(s, "f", "f\n")?
  uu.symlink(s, "f", "slink")?
  uu.symlink(s, "no-such-file", "src-dir/slink")?
  copied(s, ["-H", "-R", "slink", "src-dir", "dest-dir"])
  assert uu.dir_exists(s, "src-dir")? and uu.dir_exists(s, "dest-dir/src-dir")?
  uu.succeeds(uu.invoke(s, "cat", ["dest-dir/slink"])?)
  uu.fails_with_code(uu.invoke(s, "cat", ["dest-dir/src-dir/slink"])?, 1)
}

# origin: gnu cp/cp-deref.log
test test_gnu_cp_cp_deref_log { |ctx|
  let s = uu.scene(ctx)?
  for dir in ["a", "b", "c", "d"] { uu.mkdir(s, dir)? }
  uu.symlink(s, "../c", "a/c")?
  uu.symlink(s, "../c", "b/c")?
  copied(s, ["-RL", "a", "b", "d"])
  assert fs.stat(uu.at(s, "a/c"), follow_symlinks: true)?.kind == "dir" and fs.stat(uu.at(s, "b/c"), follow_symlinks: true)?.kind == "dir"
}

# origin: gnu cp/cp-i.log
test test_gnu_cp_cp_i_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "b/a/c")?
  uu.touch(s, "a/c")?
  uu.fails_with_code(uu.invoke(s, "cp", ["-iR", "a", "b"], stdin: b"n\n")?, 1)
  uu.touch(s, "c")?
  uu.touch(s, "d")?
  for case in [{option: "-vi", answer: "n\n", status: 1, out: ""}, {option: "-vi", answer: "y\n", status: 0, out: "'c' -> 'd'\n"}, {option: "-vni", answer: "y\n", status: 0, out: "'c' -> 'd'\n"}, {option: "-vin", answer: "y\n", status: 0, out: ""}, {option: "-in", answer: "y\n", status: 0, out: ""}, {option: "-vfi", answer: "y\n", status: 0, out: "'c' -> 'd'\n"}, {option: "-vfn", answer: "n\n", status: 0, out: ""}, {option: "-vnf", answer: "n\n", status: 0, out: ""}] {
    let r = uu.invoke(s, "cp", [case.option, "c", "d"], stdin: bytes.from_text(case.answer))?
    uu.fails_with_code(r, case.status)
    uu.stdout_is(r, case.out)
    if case.option == "-in" { uu.no_stderr(r) }
  }
  for options in [["-bn"], ["-b", "--update=none"], ["-b", "--update=none-fail"]] {
    uu.fails_with_code(uu.invoke(s, "cp", options.extend(["c", "d"]))?, 1)
  }
  uu.write(s, "old", "old\n")?
  fs.set_times(uu.at(s, "old"), mtime_sec: time.now() / 1000 - 86400)?
  uu.write(s, "new", "new\n")?
  for option in ["--update=older", "--update=all", "-u"] {
    let r = uu.invoke(s, "cp", ["-vi", option, "new", "old"], stdin: b"n\n")?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
  }
  for options in [["-v", "-i", "--update=none"], ["-v", "--update=none", "-i"], ["-v", "-n", "--update=none", "-i"]] {
    let r = uu.invoke(s, "cp", options.extend(["new", "old"]))?
    uu.succeeds(r)
    uu.no_stdout(r)
  }
}

# origin: gnu cp/cp-mv-backup.log
test test_gnu_cp_cp_mv_backup_log { |ctx|
  for util in ["cp", "mv"] {
    for initial in [["x"], ["x", "y"], ["x", "y", "y~"], ["x", "y", "y.~1~"], ["x", "y", "y~", "y.~1~"]] {
      for option in ["none", "off", "numbered", "t", "existing", "nil", "simple", "never"] {
        let s = uu.scene(ctx)?
        for name in initial { uu.touch(s, name)? }
        uu.succeeds(uu.invoke(s, util, [f"--backup={option}", "x", "y"], umask: 0o022)?)
        let has_destination = "y" in initial
        let numbered = option in ["numbered", "t"] or (option in ["existing", "nil"] and "y.~1~" in initial)
        let backing = has_destination and !(option in ["none", "off"])
        for name in ["x", "y", "y~", "y.~1~", "y.~2~"] {
          let expected = if name == "x" { util == "cp" } else if name == "y" { true } else if name == "y~" { name in initial or (backing and !numbered) } else if name == "y.~1~" { name in initial or (backing and numbered and !("y.~1~" in initial)) } else { backing and numbered and "y.~1~" in initial }
          assert uu.exists(s, name)? == expected, f"{util} {initial.join(" ")} {option}: {name}"
        }
      }
    }
  }
}

# origin: gnu cp/debug.log
test test_gnu_cp_debug_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  let r = uu.invoke(s, "cp", ["--debug", "file", "file.cp"])?
  uu.succeeds(r)
  assert regex.compile("copy offload:.*reflink:.*sparse detection:")?.matches(r.stdout.utf8()?)
  let attributes = uu.invoke(s, "cp", ["--debug", "--attributes-only", "file", "file.cp"])?
  uu.succeeds(attributes)
  assert !regex.compile("copy offload:.*reflink:.*sparse detection:")?.matches(attributes.stdout.utf8()?)
  uu.touch(s, "file.cp")?
  let skipped = uu.invoke(s, "cp", ["--debug", "--update=none", "file", "file.cp"])?
  uu.succeeds(skipped)
  uu.stdout_contains(skipped, "skipped")
  let full = uu.invoke(s, "cp", ["file", "file.cp2", "--debug"], stdout: p"/dev/full")?
  uu.fails_with_code(full, 1)
  assert uu.exists(s, "file.cp2")?
}

# origin: gnu cp/deref-slink.log
test test_gnu_cp_deref_slink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.touch(s, "slink-target")?
  uu.symlink(s, "slink-target", "slink")?
  copied(s, ["-d", "f", "slink"])
}

# origin: gnu cp/dir-rm-dest.log
test test_gnu_cp_dir_rm_dest_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.mkdir(s, "e")?
  copied(s, ["-R", "--remove-destination", "d", "e"])
  copied(s, ["-R", "--remove-destination", "d", "e"])
  uu.symlink(s, "loop", "loop")?
  uu.touch(s, "file")?
  copied(s, ["--remove-destination", "file", "loop"])
}

# origin: gnu cp/dir-slash.log
test test_gnu_cp_dir_slash_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.mkdir(s, "dir2")?
  uu.touch(s, "dir1/file")?
  copied(s, ["-R", "dir1/", "dir2"])
  assert !uu.exists(s, "dir2/file")?
  assert uu.exists(s, "dir2/dir1/file")? and uu.exists(s, "dir1/file")?
}

# origin: gnu cp/dir-vs-file.log
test test_gnu_cp_dir_vs_file_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "file")?
  uu.fails_with_code(uu.invoke(s, "cp", ["-R", "dir", "file"])?, 1)
  assert uu.file_exists(s, "file")?
}

# origin: gnu cp/existing-perm-dir.log
test test_gnu_cp_existing_perm_dir_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "src/dir")?
  uu.mkdir(s, "dst/dir")?
  uu.set_mode(s, "src/dir", 0o775)?
  uu.set_mode(s, "dst/dir", 0o700)?
  copied(s, ["-r", "src/.", "dst/"], 0o002)
  assert uu.mode(s, "dst/dir")? == 0o700
}

# origin: gnu cp/file-perm-race.log
test test_gnu_cp_file_perm_race_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  let r = wrapped(s, ["-p", "--copy-contents", "fifo", "fifo-copy"], r"""
"$@" & worker=$!; trap 'kill "$worker" 2>/dev/null; wait "$worker" 2>/dev/null' EXIT; { until test -f fifo-copy; do printf 'foo\n'; done; stat -c %a fifo-copy > observed; printf 'foo\n'; } > fifo; wait "$worker"; result=$?; trap - EXIT; exit "$result"
""", 0o022)?
  uu.succeeds(r)
  let mode = (uu.read_text(s, "observed")?).trim().parse_int()?
  assert mode % 100 == 0, f"destination exposed mode {mode} before preservation"
}

# origin: gnu cp/into-self.log
test test_gnu_cp_into_self_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "dir")?
  for args in [["-R", "dir", "dir"], ["-rl", "dir", "dir"], ["-rl", "a", "dir", "dir"], ["-rl", "a", "dir", "dir"]] {
    let r = uu.invoke(s, "cp", args)?
    uu.fails(r)
    uu.stderr_is(r, "cp: cannot copy a directory, 'dir', into itself, 'dir/dir'\n")
  }
}

# origin: gnu cp/keep-directory-symlink.log
test test_gnu_cp_keep_directory_symlink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.mkdir(s, "b/d/e")?
  uu.symlink(s, "b", "a/d")?
  let rejected = uu.invoke(s, "cp", ["-RT", "--copy-contents", "b", "a"])?
  uu.fails_with_code(rejected, 1)
  uu.stderr_only(rejected, "cp: cannot overwrite non-directory 'a/d' with directory 'b/d'\n")
  copied(s, ["-RT", "--copy-contents", "--keep-directory-symlink", "b", "a"])
  assert uu.dir_exists(s, "a/b/e")?
}

# origin: gnu cp/link-deref.log
test test_gnu_cp_link_deref_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "file")?
  uu.symlink(s, "dir", "dirlink")?
  uu.symlink(s, "file", "filelink")?
  uu.symlink(s, "nowhere", "danglink")?
  for source in ["dirlink", "filelink", "danglink"] {
    for option in ["", "-L", "-H", "-P"] {
      for recursive in [false, true] {
        let options = ["--link"].extend(if option == "" { [] } else { [option] }).extend(if recursive { ["-R"] } else { [] })
        let r = uu.invoke(s, "cp", options.extend([source, "dst"]))?
        if option == "-P" {
          uu.succeeds(r)
          uu.no_stderr(r)
          assert inode(s, "dst")? == inode(s, source)?
          assert fs.stat(uu.at(s, "dst"))?.kind == fs.stat(uu.at(s, source))?.kind
        } else if source == "danglink" {
          uu.fails_with_code(r, 1)
          uu.stderr_is(r, "cp: cannot stat 'danglink': No such file or directory\n")
          assert !uu.exists(s, "dst")?
        } else if source == "dirlink" and !recursive {
          uu.fails_with_code(r, 1)
          uu.stderr_is(r, "cp: -r not specified; omitting directory 'dirlink'\n")
          assert !uu.exists(s, "dst")?
        } else {
          uu.succeeds(r)
          uu.no_stderr(r)
          if source == "filelink" {
            assert inode(s, "dst")? == inode(s, "file")?
            assert fs.stat(uu.at(s, "dst"))?.kind == fs.stat(uu.at(s, "file"))?.kind
          }
        }
        uu.remove(s, "dst")?
      }
    }
  }
}

# origin: gnu cp/link-no-deref.log
test test_gnu_cp_link_no_deref_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "no-such-file", "dangling-slink")?
  copied(s, ["--link", "--no-dereference", "dangling-slink", "d2"])
}

# origin: gnu cp/link-preserve.log
test test_gnu_cp_link_preserve_log { |ctx|
  let s = uu.scene(ctx)?
  for case in [{tree: false, symbolic: false, args: ["-d"], same: true}, {tree: false, symbolic: true, args: ["--preserve=links", "-R", "-H"], same: true}, {tree: true, symbolic: true, args: ["--preserve=links", "-R", "-L"], same: true}, {tree: true, symbolic: false, args: ["--preserve=links", "-R", "-L"], same: true}, {tree: true, symbolic: false, args: ["-dR", "--no-preserve=links"], same: false}, {tree: false, symbolic: false, args: ["-d"], same: true}] {
    for item in ["a", "b", "c", "d"] { uu.remove(s, item)? }
    let prefix = if case.tree { "d/" } else { "" }
    if case.tree { uu.mkdir(s, "d")? } else { uu.mkdir(s, "c")? }
    uu.touch(s, f"{prefix}a")?
    if case.symbolic { uu.symlink(s, "a", f"{prefix}b")? } else { uu.hard_link(s, f"{prefix}a", f"{prefix}b")? }
    copied(s, case.args.extend(if case.tree { ["d", "c"] } else { ["a", "b", "c"] }))
    assert uu.file_exists(s, "c/a")? and uu.file_exists(s, "c/b")?
    let same_inode = inode(s, "c/a")? == inode(s, "c/b")?
    assert same_inode == case.same
  }
  for item in ["a", "b", "c", "d"] { uu.remove(s, item)? }
  uu.touch(s, "a")?
  uu.set_mode(s, "a", 0o731)?
  copied(s, ["-a", "--no-preserve=mode", "a", "b"], 0o077)
  assert uu.mode(s, "b")? == 0o600
}

# origin: gnu cp/link-symlink.log
test test_gnu_cp_link_symlink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.symlink(s, "file", "link")?
  fs.set_times(uu.at(s, "link"), mtime_sec: 1293840000)?
  copied(s, ["-al", "link", "link.cp"])
  assert fs.stat(uu.at(s, "link.cp"))?.mtime_ns == 1293840000000000000
}

# origin: gnu cp/link.log
test test_gnu_cp_link_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["src", "dest", "dest2"] { uu.touch(s, name)? }
  copied(s, ["-f", "--link", "src", "dest"])
  copied(s, ["-f", "--symbolic-link", "src", "dest2"])
}

# origin: gnu cp/no-deref-link1.log
test test_gnu_cp_no_deref_link1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "b")?
  uu.write(s, "a/foo", "bar\n")?
  uu.symlink(s, "../a/foo", "b/foo")?
  uu.fails_with_code(uu.invoke(s, "cp", ["-d", "a/foo", "b"])?, 1)
  uu.file_is(s, "a/foo", "bar\n")
}

# origin: gnu cp/no-deref-link2.log
test test_gnu_cp_no_deref_link2_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "b")?
  uu.write(s, "a", "bar\n")?
  uu.symlink(s, "../a", "b/a")?
  uu.fails_with_code(uu.invoke(s, "cp", ["-d", "a", "b"])?, 1)
  uu.file_is(s, "a", "bar\n")
}

# origin: gnu cp/no-deref-link3.log
test test_gnu_cp_no_deref_link3_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "bar\n")?
  uu.symlink(s, "a", "b")?
  uu.fails_with_code(uu.invoke(s, "cp", ["-d", "a", "b"])?, 1)
  uu.file_is(s, "a", "bar\n")
}

# origin: gnu cp/parent-perm-race.log
test test_gnu_cp_parent_perm_race_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["mode", "ownership", "d"] { uu.mkdir(s, name)?; uu.set_mode(s, name, 0o775)? }
  uu.set_mode(s, "d", 0o2775)?
  for attr in ["mode", "ownership"] {
    uu.mkfifo(s, f"{attr}/fifo")?
    let r = wrapped(s, [f"--preserve={attr}", "-R", "--copy-contents", "--parents", attr, "d"], f"\"$@\" & worker=$!; trap 'kill \"$worker\" 2>/dev/null; wait \"$worker\" 2>/dev/null' EXIT; timeout 10 sh -c 'ls -ld d/{attr} > {attr}/fifo' || exit 1; wait \"$worker\"; result=$?; trap - EXIT; exit \"$result\"", 0o002)?
    uu.succeeds(r)
    let observed = uu.read_text(s, f"d/{attr}/fifo")?
    let pattern = if attr == "ownership" { "^d...--[-S]--[-S]" } else { "^d....-..-.|^d..[-x].w[-x].-[-x]" }
    assert regex.compile(pattern)?.matches(observed), f"unsafe temporary directory permissions: {observed}"
  }
}

# origin: gnu cp/parent-perm.log
test test_gnu_cp_parent_perm_log { |ctx|
  let s = uu.scene(ctx)?
  for dir in ["a/b/c", "a/b/d", "e"] { uu.mkdir(s, dir)? }
  uu.touch(s, "a/b/c/foo")?
  uu.touch(s, "a/b/d/foo")?
  copied(s, ["-p", "--parent", "a/b/c/foo", "e"])
  for dir in ["e/a", "e/a/b"] { uu.set_mode(s, dir, uu.mode(s, dir)? - 0o050)? }
  copied(s, ["-p", "--parent", "a/b/d/foo", "e"])
  for dir in ["a", "a/b", "a/b/d"] {
    let source = uu.mode(s, dir)? % 0o1000
    let destination = uu.mode(s, f"e/{dir}")? % 0o1000
    assert source == destination
  }
}

# origin: gnu cp/perm.log
test test_gnu_cp_perm_log { |ctx|
  let s = uu.scene(ctx)?
  for mask in [0o031, 0o037, 0o002] {
    for command in ["mv", "preserve", "cp"] {
      for force in [false, true] {
        for existing in [true, false] {
          for group_bits in [4, 2, 1, 6, 3, 5, 7] {
            for other in [4, 2, 1, 6, 3, 5, 7] {
              if uu.exists(s, "src")? { fs.set_times(uu.at(s, "src"), atime_now: true, mtime_now: true)? } else { uu.touch(s, "src")? }
              uu.set_mode(s, "src", 0o450)?
              uu.remove(s, "dest")?
              if existing { uu.touch(s, "dest")?; uu.set_mode(s, "dest", 0o600 + group_bits * 8 + other)? }
              let util = if command == "mv" { "mv" } else { "cp" }
              let args = (if command == "preserve" { ["-p"] } else { [] }).extend(if force { ["-f"] } else { [] }).extend(["src", "dest"])
              uu.succeeds(uu.invoke(s, util, args, umask: mask)?)
              if command == "mv" { assert !uu.exists(s, "src")? }
              if command == "cp" { assert uu.file_exists(s, "src")? }
              let expected = if command != "cp" { 0o450 } else if existing { 0o600 + group_bits * 8 + other } else if mask == 0o031 { 0o440 } else if mask == 0o037 { 0o440 } else { 0o450 }
              assert uu.mode(s, "dest")? == expected, f"{command} force={force} existing={existing} mask={mask} group={group_bits} other={other}"
              if !existing { break }
            }
            if !existing { break }
          }
        }
      }
    }
  }
}

# origin: gnu cp/preserve-2.log
test test_gnu_cp_preserve_2_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  copied(s, ["--preserve=mode,links", "f", "g"])
}

# origin: gnu cp/preserve-link.log
test test_gnu_cp_preserve_link_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "s")?
  uu.touch(s, "s/f")?
  for name in ["linkm", "linke", "fileo", "fileu"] { uu.hard_link(s, "s/f", f"s/{name}")? }
  for first in ["f", "linkm"] {
    uu.remove(s, "t")?
    uu.mkdir(s, "t/s")?
    uu.touch(s, f"t/s/{first}")?
    uu.hard_link(s, f"t/s/{first}", "t/s/linke")?
    uu.touch(s, "t/s/fileo")?
    uu.touch(s, "t/s/fileu")?
    fs.set_times(uu.at(s, "t/s/fileo"), mtime_sec: time.now() / 1000 - 3600)?
    fs.set_times(uu.at(s, "t/s/fileu"), mtime_sec: time.now() / 1000 + 3600)?
    copied(s, ["-au", "s", "t"])
    for name in ["linkm", "linke", "fileo", "fileu"] { assert inode(s, "t/s/f")? == inode(s, f"t/s/{name}")? }
  }
}

# origin: gnu cp/preserve-mode.log
test test_gnu_cp_preserve_mode_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "b"] { uu.touch(s, name)?; uu.set_mode(s, name, 0o644)? }
  uu.set_mode(s, "b", 0o600)?
  copied(s, ["--no-preserve=mode", "b", "c"], 0o022)
  assert uu.mode(s, "a")? == uu.mode(s, "c")?
  uu.set_mode(s, "c", 0o600)?
  copied(s, ["--no-preserve=mode", "a", "b"], 0o022)
  assert uu.mode(s, "b")? == uu.mode(s, "c")?
  for name in ["d1", "d2"] { uu.mkdir(s, name)?; uu.set_mode(s, name, 0o755)? }
  uu.set_mode(s, "d2", 0o705)?
  copied(s, ["--no-preserve=mode", "-r", "d2", "d3"], 0o022)
  assert uu.mode(s, "d1")? == uu.mode(s, "d3")?
  for name in ["a", "b"] { uu.remove(s, name)? }
  uu.touch(s, "a")?
  uu.set_mode(s, "a", 0o600)?
  copied(s, ["--no-preserve=mode", "--preserve=all", "a", "b"], 0o022)
  assert uu.mode(s, "a")? == uu.mode(s, "b")?
  uu.mkfifo(s, "fifo")?
  copied(s, ["-a", "--no-preserve=mode", "fifo", "fifo_copy"], 0o022)
  assert uu.mode(s, "fifo")? == uu.mode(s, "fifo_copy")?
  for name in ["a", "b", "c"] { uu.remove(s, name)? }
  uu.touch(s, "a")?
  uu.set_mode(s, "a", 0o660)?
  copied(s, ["a", "b"], 0o022)
  copied(s, ["--preserve=ownership", "a", "c"], 0o022)
  assert uu.mode(s, "b")? == uu.mode(s, "c")?
  for name in ["a", "b"] { uu.remove(s, name)? }
  uu.write(s, "a", "not-writable-dest\n")?
  uu.set_mode(s, "a", 0o644)?
  copied(s, ["a", "b"], 0o377)
  assert uu.read(s, "a")? == uu.read(s, "b")?
  assert uu.mode(s, "b")? == 0o400
}

# origin: gnu cp/preserve-slink-time.log
test test_gnu_cp_preserve_slink_time_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "no-such", "dangle")?
  copied(s, ["-Pp", "dangle", "d2"])
  assert fs.stat(uu.at(s, "dangle"))?.mtime_ns == fs.stat(uu.at(s, "d2"))?.mtime_ns
}

# origin: gnu cp/proc-short-read.log
test test_gnu_cp_proc_short_read_log { |ctx|
  let s = uu.scene(ctx)?
  copied(s, ["/proc/cpuinfo", "1"])
  let reference = p"/proc/cpuinfo".read_text()?
  let filter = regex.compile("MHz|[Bb][Oo][Gg][Oo][Mm][Ii][Pp][Ss]")?
  let copied_lines = [line for line in (uu.read_text(s, "1")?).split("\n") if !filter.matches(line)]
  let reference_lines = [line for line in reference.split("\n") if !filter.matches(line)]
  assert copied_lines == reference_lines
}

# origin: gnu cp/proc-zero-len.log
test test_gnu_cp_proc_zero_len_log { |ctx|
  let s = uu.scene(ctx)?
  let reference = p"/proc/cpuinfo".read_bytes()?
  copied(s, ["/proc/cpuinfo", "exp"])
  assert (uu.size(s, "exp")? > 0) == (reference.len() > 0)
}

# origin: gnu cp/r-vs-symlink.log
test test_gnu_cp_r_vs_symlink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "abc\n")?
  uu.symlink(s, "foo", "slink")?
  uu.symlink(s, "no-such-file", "no-file")?
  copied(s, ["-r", "no-file", "junk"])
  copied(s, ["-r", "slink", "bar"])
  assert uu.is_symlink(s, "bar")?
}

# origin: gnu cp/readonly-dir.log
test test_gnu_cp_readonly_dir_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b/c/d")?
  uu.write(s, "a/b/c/d/bar.txt", "test content\n")?
  for name in ["a", "a/b", "a/b/c", "a/b/c/d"] { uu.set_mode(s, name, 0o555)? }
  uu.set_mode(s, "a/b/c/d/bar.txt", 0o444)?
  defer restore_writable(s)
  copied(s, ["-r", "a", "b"], 0o022)
  for name in ["b", "b/b", "b/b/c", "b/b/c/d"] { assert uu.mode(s, name)? == 0o555 }
  copied(s, ["-a", "a", "c"], 0o022)
  for name in ["c", "c/b"] { assert uu.mode(s, name)? == 0o555 }
}

# origin: gnu cp/reflink-auto.log
test test_gnu_cp_reflink_auto_log { |ctx|
  let s = uu.scene(ctx)?
  let other_root = fs.tempdir_in(p"/dev/shm")?
  defer other_root.close()?
  let other = other_root.host_path()?
  let source = fp"{other}/a"
  source.write("non_zero_size\n")?
  assert fs.stat(source)?.dev != fs.stat(s.root)?.dev
  uu.fails_with_code(uu.invoke(s, "cp", ["--reflink", source.display(), "b"])?, 1)
  for options in [["--reflink=auto"], ["--reflink=auto", "--sparse=always"], ["--reflink=auto", "--reflink=never"]] {
    copied(s, options.extend([source.display(), "b"]))
    assert uu.size(s, "b")? > 0
  }
}

# origin: gnu cp/reflink-perm.log
test test_gnu_cp_reflink_perm_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  fs.set_times(uu.at(s, "file"), mtime_sec: 1251486000)?
  uu.set_mode(s, "file", 0o777)?
  copied(s, ["--reflink=auto", "--preserve", "file", "copy"], 0o077)
  assert uu.mode(s, "copy")? == 0o777
  assert fs.stat(uu.at(s, "copy"))?.mtime_ns <= fs.stat(uu.at(s, "file"))?.mtime_ns
  uu.write(s, "file2", "\n")?
  for option in ["auto", "always"] {
    copied(s, [f"--reflink={option}", "--preserve", "--attributes-only", "file2", "empty_copy"], 0o077)
    assert uu.read(s, "empty_copy")? == b""
  }
}

proc restore_writable(s: uu.Scene) [fs, process, error] {
  for name in ["a", "b", "c"] {
    if uu.exists(s, name)? { let _ = host(s, ["chmod", "-R", "u+w", name])? }
  }
}

# origin: gnu cp/slink-2-slink.log
test test_gnu_cp_slink_2_slink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  for name in ["a", "b"] { uu.symlink(s, "file", name)? }
  for name in ["c", "d"] { uu.symlink(s, "no-such-file", name)? }
  copied(s, ["--update", "--no-dereference", "a", "b"])
  copied(s, ["--update", "--no-dereference", "c", "d"])
}

# origin: gnu cp/sparse-2.log
test test_gnu_cp_sparse_2_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "k", "x")?
  uu.truncate(s, "k", 128 * 1024)?
  for append in [false, true] {
    if append { uu.append(s, "k", "y")? }
    for sparse in ["always", "never"] {
      copied(s, ["--reflink=never", f"--sparse={sparse}", "k", "k2"])
      assert uu.read(s, "k")? == uu.read(s, "k2")?
    }
  }
  uu.remove(s, "k")?
  uu.write(s, "k", "x")?
  let _ = bytes.write_at(uu.at(s, "k"), 1024, bytes.zero(255 * 1024)?)?
  let detected = uu.invoke(s, "cp", ["--debug", "--reflink=never", "--sparse=always", "k", "k2"])?
  uu.succeeds(detected)
  assert uu.read(s, "k")? == uu.read(s, "k2")?
  assert regex.compile("sparse detection: .*zeros")?.matches(detected.stdout.utf8()?)
  let dense = uu.invoke(s, "cp", ["--debug", "--sparse=never", "k", "k2"])?
  uu.succeeds(dense)
  assert uu.read(s, "k")? == uu.read(s, "k2")?
  uu.stdout_contains(dense, "copy offload: avoided, reflink: no")
}

# origin: gnu cp/sparse-extents-2.log
test test_gnu_cp_sparse_extents_2_log { |ctx|
  let s = uu.scene(ctx)?
  for width in [2 * n + 1 for n in range(0, 11)] {
    for count in [1, 2, 31, 100] {
      uu.touch(s, "j1")?
      let chunk = width * 1024
      for index in range(1, count + 1) {
        let data = bytes.from_ints([index for _ in range(0, chunk)])?
        let _ = bytes.write_at(uu.at(s, "j1"), (2 * index - 1) * chunk, data)?
      }
      copied(s, ["--reflink=never", "--sparse=always", "j1", "j2"])
      assert uu.read(s, "j1")? == uu.read(s, "j2")?, f"extent data width={width} count={count}"
      uu.remove(s, "j1")?
      uu.remove(s, "j2")?
    }
  }
}

# origin: gnu cp/sparse-extents.log
test test_gnu_cp_sparse_extents_log { |ctx|
  let s = uu.scene(ctx)?
  for sparse in ["always", "auto", "never"] {
    for allocation in [["-l", "4194304"], ["-l", "1048576", "-o", "4194304"], ["-l", "1"]] {
      let random = host(s, ["dd", "count=10", "if=/dev/urandom", "iflag=fullblock", "of=unwritten.withdata"])?
      uu.truncate(s, "unwritten.withdata", 2 * 1024 * 1024)?
      let _ = host(s, ["fallocate"].extend(allocation).extend(["-n", "unwritten.withdata"]))?
      copied(s, ["--reflink=never", f"--sparse={sparse}", "unwritten.withdata", "cp.test"])
      assert uu.size(s, "unwritten.withdata")? == uu.size(s, "cp.test")?
      assert uu.read(s, "unwritten.withdata")? == uu.read(s, "cp.test")?
      uu.remove(s, "unwritten.withdata")?
      uu.remove(s, "cp.test")?
    }
  }
}

# origin: gnu cp/sparse-perf.log
test test_gnu_cp_sparse_perf_log { |ctx|
  let s = uu.scene(ctx)?
  let other_root = fs.tempdir_in(p"/dev/shm")?
  defer other_root.close()?
  let other = other_root.host_path()?
  let source = fp"{other}/k"
  source.write("x")?
  source.truncate(1024 * 1024)?
  assert fs.stat(source)?.dev != fs.stat(s.root)?.dev
  let cross_device = uu.invoke(s, "cp", ["--debug", source.display(), "k2"])?
  uu.succeeds(cross_device)
  assert source.read_bytes()? == uu.read(s, "k2")?
  assert !(": avoided" in cross_device.stdout.utf8()?)
  uu.write(s, "might-look-sparse", ["y\n" for _ in range(0, 1024 * 1024)].join(""))?
  let compressible = uu.invoke(s, "cp", ["--debug", "might-look-sparse", "might-look-sparse.cp"])?
  uu.succeeds(compressible)
  assert uu.read(s, "might-look-sparse")? == uu.read(s, "might-look-sparse.cp")?
  assert !(": avoided" in compressible.stdout.utf8()?)
  uu.touch(s, "f")?
  uu.truncate(s, "f", 1024 * 1024 * 1024 * 1024)?
  let large = uu.invoke(s, "cp", ["--reflink=never", "f", "f2"], timeout: 10s)?
  uu.succeeds(large)
  assert uu.size(s, "f")? == uu.size(s, "f2")?
}

# origin: gnu cp/sparse-to-pipe.log
test test_gnu_cp_sparse_to_pipe_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "pipe")?
  uu.touch(s, "sparse")?
  uu.truncate(s, "sparse", 1024 * 1024)?
  let plan = uu.command(s, "cat", ["pipe"], stdout: uu.at(s, "copy"), stderr: uu.at(s, "cat.err"), timeout: 10s)?
  let reader = spawn plan?
  defer reader.cancel(signal: "KILL", kill_after: 0ms)?
  let r = uu.invoke(s, "cp", ["-T", "sparse", "pipe"], timeout: 10s)?
  uu.succeeds(r)
  let done = process.wait_timeout([reader], 10s)?
  assert done != null
  assert done.status.exited_with(0)
  assert uu.read(s, "sparse")? == uu.read(s, "copy")?
}

# origin: gnu cp/sparse.log
test test_gnu_cp_sparse_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "sparse")?
  uu.truncate(s, "sparse", 128 * 1024 + 1)?
  copied(s, ["--reflink=never", "--sparse=always", "sparse", "copy"])
  assert fs.stat(uu.at(s, "copy"))?.blocks_512 <= fs.stat(uu.at(s, "sparse"))?.blocks_512
  for option in ["always", "never"] {
    uu.fails_with_code(uu.invoke(s, "cp", [f"--sparse={option}", "--reflink", "sparse", "copy"])?, 1)
  }
  let hole_size = fs.stat(uu.at(s, "copy"))?.blksize
  for data_first in [true, false] {
    for chunks in [1, 2, 4, 11, 32, 128] {
      let part_count = 128 / chunks
      let data = bytes.from_text(["U" for _ in range(0, chunks * hole_size)].join(""))
      let hole = bytes.zero(chunks * hole_size)?
      let parts = [if index % 2 == 0 == data_first { data } else { hole } for index in range(0, part_count)]
      uu.write_bytes(s, "file.in", bytes.concat(parts))?
      copied(s, ["--reflink=never", "--sparse=always", "file.in", "sparse.out"])
      copied(s, ["--reflink=never", "--sparse=always", "sparse.out", "sparse.out2"])
      assert uu.read(s, "file.in")? == uu.read(s, "sparse.out")?
      assert uu.read(s, "file.in")? == uu.read(s, "sparse.out2")?
    }
  }
}

# origin: gnu cp/special-f.log
test test_gnu_cp_special_f_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  uu.touch(s, "e")?
  for options in [["-R"], ["-R", "-f"]] {
    uu.succeeds(uu.invoke(s, "cp", options.extend(["fifo", "e"]), timeout: 10s)?)
    assert fs.stat(uu.at(s, "fifo"))?.kind == "fifo"
  }
}

# origin: gnu cp/src-base-dot.log
test test_gnu_cp_src_base_dot_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "x")?
  uu.mkdir(s, "y")?
  let r = wrapped(s, ["--verbose", "-ab", "../x/.", "."], "cd y && exec \"$@\" >out 2>&1")?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.read(s, "y/out")? == b""
}

# origin: gnu cp/symlink-slash.log
test test_gnu_cp_symlink_slash_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.symlink(s, "dir", "symlink")?
  copied(s, ["-dR", "symlink/", "s"])
  assert uu.dir_exists(s, "s")? and !uu.is_symlink(s, "s")?
  assert (fs.children(uu.at(s, "s"))? |> count()) == 0
}

# origin: gnu cp/thru-dangling.log
test test_gnu_cp_thru_dangling_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "no-such", "dangle")?
  uu.write(s, "f", "hi\n")?
  for options in [[], ["-f"]] {
    let r = uu.invoke(s, "cp", options.extend(["f", "dangle"]))?
    uu.fails_with_code(r, 1)
    uu.stderr_only(r, "cp: not writing through dangling symlink 'dangle'\n")
    assert !uu.exists(s, "no-such")?
  }
  let historical = uu.invoke(s, "cp", ["f", "dangle"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(historical)
  uu.no_output(historical)
  uu.file_is(s, "no-such", "hi\n")
  uu.symlink(s, "loop", "loop")?
  let forced = uu.invoke(s, "cp", ["-f", "f", "loop"])?
  uu.succeeds(forced)
  uu.no_output(forced)
  uu.file_is(s, "loop", "hi\n")
  assert uu.file_exists(s, "loop")?
}

# origin: gnu cp/same-file.log
test test_gnu_cp_same_file_log { |ctx|
  let options = ["", "-d", "-f", "-df", "--rem", "-b", "-bd", "-bf", "-bdf", "-l", "-dl", "-fl", "-dfl", "-bl", "-bdl", "-bfl", "-bdfl", "-s", "-sf"]
  let matrices = [
    {source: "foo", destination: "symlink", statuses: [1,1,1,1,0,0,0,0,0,1,1,0,0,0,0,0,0,1,0], linked: [0,1,2,3,9,10,17,18], backups: [5,6,7,8,13,14,15,16]},
    {source: "symlink", destination: "foo", statuses: [1,1,1,1,1,1,0,1,0,0,0,0,-1,0,-1,0,-1,1,1], linked: [6,8], backups: [6,8]},
    {source: "foo", destination: "foo", statuses: [1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,0,0,1,1], linked: [], backups: [7,8,15,16]},
    {source: "sl1", destination: "sl2", statuses: [1,0,1,0,0,0,0,0,0,1,-1,0,-1,0,-1,0,-1,1,0], linked: [0,1,2,3,6,8,9,17,18], backups: [5,6,7,8,13,15]},
    {source: "foo", destination: "hardlink", statuses: [1,1,1,1,0,0,0,0,0,0,0,0,0,0,0,0,0,1,1], linked: [], backups: [5,6,7,8]},
    {source: "hlsl", destination: "sl2", statuses: [1,0,1,0,0,0,0,0,0,1,0,0,0,0,0,0,0,1,0], linked: [0,1,2,3,6,8,9,10,12,14,16,17,18], backups: [5,6,7,8,13,15]},
  ]
  for matrix in matrices {
    for index in range(0, options.len()) {
      if matrix.statuses[index] == -1 { continue }
      let s = uu.scene(ctx)?
      uu.write(s, "foo", "XYZ\n")?
      var initial = ["foo"]
      if matrix.source == "symlink" or matrix.destination == "symlink" { uu.symlink(s, "foo", "symlink")?; initial += ["symlink"] }
      if matrix.destination == "hardlink" { uu.hard_link(s, "foo", "hardlink")?; initial += ["hardlink"] }
      if matrix.source == "sl1" { uu.symlink(s, "foo", "sl1")?; initial += ["sl1"] }
      if matrix.destination == "sl2" { uu.symlink(s, "foo", "sl2")?; initial += ["sl2"] }
      if matrix.source == "hlsl" { uu.hard_link(s, "sl2", "hlsl")?; initial += ["hlsl"] }
      let args = (if options[index] == "" { [] } else { [options[index]] }).extend([matrix.source, matrix.destination])
      let r = uu.invoke(s, "cp", args, vars: {VERSION_CONTROL: "numbered"})?
      uu.fails_with_code(r, matrix.statuses[index])
      uu.no_stdout(r)
      if r.status == 0 { uu.no_stderr(r) } else {
        let detail = if index in [9,10] { f"cannot create hard link '{matrix.destination}' to '{matrix.source}'" } else if index == 17 and matrix.source != "symlink" and matrix.destination != "foo" and matrix.destination != "hardlink" { f"cannot create symbolic link '{matrix.destination}' to '{matrix.source}'" } else { f"'{matrix.source}' and '{matrix.destination}' are the same file" }
        let diagnostic = r.stderr.utf8()?.trim().split(":")[1].trim().replace("symbolic link", with: "symlink")
        assert diagnostic == detail.replace("symbolic link", with: "symlink"), f"{matrix.source} {matrix.destination} {options[index]}: {diagnostic}"
      }
      let linked = index in matrix.linked
      assert uu.is_symlink(s, matrix.destination)? == linked
      if linked {
        let target = if index == 18 and matrix.destination == "sl2" { matrix.source } else { "foo" }
        assert uu.read_link(s, matrix.destination)? == target
      }
      for name in initial {
        if name == matrix.destination { continue }
        let source_link = name in ["symlink", "sl1", "sl2", "hlsl"]
        assert uu.is_symlink(s, name)? == source_link
        if source_link { assert uu.read_link(s, name)? == "foo" }
      }
      let backup = f"{matrix.destination}.~1~"
      let backed_up = index in matrix.backups
      assert present(s, backup)? == backed_up
      if backed_up {
        let backup_link = matrix.destination in ["symlink", "sl2"]
        assert uu.is_symlink(s, backup)? == backup_link
        if backup_link { assert uu.read_link(s, backup)? == "foo" } else { uu.file_is(s, backup, "XYZ\n") }
      }
      for operand in [matrix.source, matrix.destination] {
        if matrix.source == "symlink" and index in [6,8] { assert fs.stat(uu.at(s, operand), follow_symlinks: true) is Err(_) } else { uu.file_is(s, operand, "XYZ\n") }
      }
      let names = fs.children(s.root)? |> map .name |> collect()
      let expected_names = initial.extend(if backed_up { [backup] } else { [] }) |> sort() |> collect()
      assert names == expected_names
    }
  }
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "XYZ\n")?
  let r = uu.invoke(s, "cp", ["--remove-destination", "foo", "./foo"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "are the same file")
  assert uu.file_exists(s, "foo")?
  uu.file_is(s, "foo", "XYZ\n")
}

proc present(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  match fs.stat(uu.at(s, name)) {
    Ok(_) => Ok(true),
    Err(is NotFound) => Ok(false),
    Err(failure) => Err(failure),
  }
}
