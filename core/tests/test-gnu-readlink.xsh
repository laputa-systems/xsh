use support.uu

# origin: gnu readlink/multi.log
test test_gnu_readlink_multi_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "regfile")?
  uu.symlink(s, "regfile", "link1")?
  uu.succeeds(uu.invoke(s, "readlink", ["link1", "link1"])?)
  uu.fails_with_code(uu.invoke(s, "readlink", ["link1", "link2"])?, 1)
  uu.fails_with_code(uu.invoke(s, "readlink", ["link1", "link2", "link1"])?, 1)
  uu.succeeds(uu.invoke(s, "readlink", ["-m", "link1", "link2"])?)
  for args in [["-m", "--zero", "/1", "/1"], ["-n", "-m", "--zero", "/1", "/1"]] {
    let r = uu.invoke(s, "readlink", args)?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, b"/1\0/1\0")
  }
  let out = uu.at(s, "out")
  for operands in [["/1", "/1"], ["/1"]] {
    uu.succeeds(uu.invoke(s, "readlink", ["-n", "-m", "--zero"].extend(operands), stdout: out, stdout_append: true)?)
  }
  assert out.read_bytes()? == b"/1\0/1\0/1"
}

# origin: gnu readlink/readlink-fp-loop.log
test test_gnu_readlink_readlink_fp_loop_log { |ctx|
  let s = uu.scene(ctx)?
  let cwd = s.root.resolve()?.display()
  uu.symlink(s, "s", "p")?
  uu.symlink(s, "d", "s")?
  uu.mkdir(s, "d")?
  uu.write(s, "d/2", "2\n")?
  uu.symlink(s, "../s/2", "d/1")?
  let finite = uu.invoke(s, "readlink", ["-v", "-e", "p/1"])?
  uu.succeeds(finite)
  uu.stdout_is(finite, cwd + "/d/2\n")
  uu.remove(s, "d/2")?
  uu.symlink(s, "../s/1", "d/2")?
  let cyclic = uu.invoke(s, "readlink", ["-v", "-e", "p/1"])?
  uu.fails(cyclic)
  let diagnostic = cyclic.stderr.utf8()?.trim()
  assert diagnostic.starts_with("readlink: p/1: ")
  let reason = diagnostic.byte_slice("readlink: p/1: ".byte_len())
  uu.remove(s, "d/2")?
  uu.symlink(s, "../s/3", "d/2")?
  for index in range(3, 8) { uu.symlink(s, f"../p/{index + 1}", f"d/{index}")? }
  uu.write(s, "d/8", "x\n")?
  let longer = uu.invoke(s, "readlink", ["-v", "-e", "p/1"])?
  uu.succeeds(longer)
  uu.stdout_is(longer, cwd + "/d/8\n")
  uu.symlink(s, "loop", "loop")?
  let direct = uu.invoke(s, "readlink", ["-v", "-e", "loop"])?
  uu.fails(direct)
  uu.stderr_is(direct, f"readlink: loop: {reason}\n")
}

# origin: gnu readlink/readlink-posix.log
test test_gnu_readlink_readlink_posix_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.symlink(s, "file", "link1")?
  let plain = uu.invoke(s, "readlink", ["file"], vars: {POSIXLY_CORRECT: "1"})?
  uu.fails_with_code(plain, 1)
  assert plain.stderr.utf8()?.replace("Argument", with: "argument") == "readlink: file: Invalid argument\n"
  for mode in ["-f", "-e", "-m"] { uu.succeeds(uu.invoke(s, "readlink", [mode, "file"], vars: {POSIXLY_CORRECT: "1"})?) }
  let link = uu.invoke(s, "readlink", ["link1"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(link)
  uu.stdout_is(link, "file\n")
  for mode in ["-f", "-e", "-m"] { uu.succeeds(uu.invoke(s, "readlink", [mode, "link1"], vars: {POSIXLY_CORRECT: "1"})?) }
}

# origin: gnu readlink/readlink-root.log
test test_gnu_readlink_readlink_root_log { |ctx|
  let s = uu.scene(ctx)?
  let single = fs.stat(p"/")?
  let double = fs.stat(p"//")?
  let double_root = if single.dev == double.dev and single.ino == double.ino { "/" } else { "//" }
  assert p"/dev".is_dir()?
  for item in [
    {name: "one", target: "/"}, {name: "two", target: "//"}, {name: "three", target: "///"},
    {name: "one-dots", target: "/./..//"}, {name: "two-dots", target: "//./..//"}, {name: "three-dots", target: "///./..//"},
    {name: "one-dev", target: "/dev"}, {name: "two-dev", target: "//dev"}, {name: "three-dev", target: "///dev"},
  ] { uu.symlink(s, item.target, item.name)? }
  for item in [
    {operand: "/", mode: "-e", expected: "/"}, {operand: "//", mode: "-e", expected: double_root}, {operand: "///", mode: "-e", expected: "/"},
    {operand: "/.//..", mode: "-e", expected: "/"}, {operand: "//.//..", mode: "-e", expected: double_root}, {operand: "///.//..", mode: "-e", expected: "/"},
    {operand: "one", mode: "-e", expected: "/"}, {operand: "two", mode: "-e", expected: double_root}, {operand: "three", mode: "-e", expected: "/"},
    {operand: "one-dots", mode: "-e", expected: "/"}, {operand: "two-dots", mode: "-e", expected: double_root}, {operand: "three-dots", mode: "-e", expected: "/"},
    {operand: "one-dev", mode: "-e", expected: "/dev"}, {operand: "two-dev", mode: "-f", expected: double_root + "dev"}, {operand: "three-dev", mode: "-e", expected: "/dev"},
    {operand: "one/dev", mode: "-e", expected: "/dev"}, {operand: "two/dev", mode: "-f", expected: double_root + "dev"}, {operand: "three/dev", mode: "-e", expected: "/dev"},
    {operand: "one-dots/dev", mode: "-e", expected: "/dev"}, {operand: "two-dots/dev", mode: "-f", expected: double_root + "dev"}, {operand: "three-dots/dev", mode: "-e", expected: "/dev"},
  ] {
    let r = uu.invoke(s, "readlink", [item.mode, item.operand])?
    uu.succeeds(r)
    uu.stdout_is(r, item.expected + "\n")
  }
}

# origin: gnu readlink/rl-1.log
test test_gnu_readlink_rl_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "subdir")?
  uu.touch(s, "regfile")?
  uu.symlink(s, "regfile", "link1")?
  uu.symlink(s, "missing", "link2")?
  uu.symlink(s, "q name", "qlink")?
  for item in [{name: "link1", expected: "regfile"}, {name: "link2", expected: "missing"}] {
    let r = uu.invoke(s, "readlink", [item.name])?
    uu.succeeds(r)
    assert r.stdout.utf8()?.trim() == item.expected
  }
  for style in ["literal", "shell-always", "invalid"] {
    let r = uu.invoke(s, "readlink", ["qlink"], vars: {QUOTING_STYLE: style})?
    uu.succeeds(r)
    uu.stdout_only(r, "q name\n")
  }
  let zero = uu.invoke(s, "readlink", ["-z", "qlink"], vars: {QUOTING_STYLE: "invalid"})?
  uu.succeeds(zero)
  uu.stdout_only_bytes(zero, b"q name\0")
  for name in ["subdir", "regfile", "missing"] {
    let r = uu.invoke(s, "readlink", [name])?
    uu.fails_with_code(r, 1)
    assert r.stdout.utf8()?.trim() == ""
  }
}
