use support.uu as uu

# origin: gnu chmod/c-option.log
test test_gnu_chmod_c_option_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.set_mode(s, "f", 0o444)?
  uu.succeeds(uu.invoke(s, "chmod", ["u=rwx", "f"], umask: 0)?)
  let changed = uu.invoke(s, "chmod", ["-c", "g=rwx", "f"], umask: 0)?
  uu.succeeds(changed)
  uu.stdout_is(changed, "mode of 'f' changed from 0744 (rwxr--r--) to 0774 (rwxrwxr--)\n")
  let unchanged = uu.invoke(s, "chmod", ["-c", "g=rwx", "f"], umask: 0)?
  uu.succeeds(unchanged)
  uu.no_stdout(unchanged)
  uu.mkdir(s, "a/b")?
  let setgid = uu.invoke(s, "chmod", ["g+s", "a/b"], umask: 0)?
  let recursive = uu.invoke(s, "chmod", ["-c", "-R", "g+w", "a"], umask: 0)?
  uu.no_stderr(recursive)
}

# origin: gnu chmod/equal-x.log
test test_gnu_chmod_equal_x_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  for mode in ["=x", "=xX", "=Xx", "=x,=X", "=X,=x"] {
    uu.succeeds(uu.invoke(s, "chmod", [f"a=r,{mode}", "f"], umask: 0o005)?)
    assert uu.mode(s, "f")? == 0o110
  }
}

# origin: gnu chmod/equals.log
test test_gnu_chmod_equals_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  for source in ["u", "g", "o"] {
    for target in ["u", "g", "o"] {
      if source != target {
        uu.succeeds(uu.invoke(s, "chmod", [f"a=,{source}=rwx,{target}={source},{source}=", "f"])?)
        assert uu.mode(s, "f")? == (if target == "u" { 0o700 } else if target == "g" { 0o070 } else { 0o007 })
      }
    }
  }
  uu.succeeds(uu.invoke(s, "chmod", ["a=,u=rwx,=u", "f"], umask: 0o027)?)
  assert uu.mode(s, "f")? == 0o750
}

# origin: gnu chmod/ignore-symlink.log
test test_gnu_chmod_ignore_symlink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/f")?
  uu.symlink(s, "f", "dir/l")?
  let r = uu.invoke(s, "chmod", ["u+w", "-R", "dir"])?
  uu.succeeds(r)
  uu.no_stderr(r)
}

# origin: gnu chmod/inaccessible.log
test test_gnu_chmod_inaccessible_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d/e")?
  defer { uu.set_mode(s, "d", 0o700)?; uu.set_mode(s, "d/e", 0o700)? }
  uu.set_mode(s, "d/e", 0)?
  uu.set_mode(s, "d", 0)?
  uu.succeeds(uu.invoke(s, "chmod", ["u+rwx", "d", "d/e"])?)
}

# origin: gnu chmod/no-x.log
test test_gnu_chmod_no_x_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d/no-x/y")?
  uu.mkdir(s, "a/b")?
  defer { uu.set_mode(s, "d", 0o700)?; uu.set_mode(s, "d/no-x", 0o700)?; uu.set_mode(s, "a", 0o700)? }
  uu.set_mode(s, "d/no-x", 0o600)?
  let r = uu.invoke(s, "chmod", ["-R", "o=r", "d"])?
  uu.fails(r)
  assert r.stderr.utf8()?.replace("cannot access ", with: "").replace("cannot read directory ", with: "").replace("d/no-x/y", with: "d/no-x") == "chmod: 'd/no-x': Permission denied\n"
  let child: uu.Scene = {ctx: ctx, root: uu.at(s, "a")}
  let words = uu.argv(child, "chmod", [p"a-x", p".", p"b"])?
  let out = uu.at(s, "out")
  let err = uu.at(s, "err")
  let status = process.run(process.command_argv(ctx.xsh_bin, words, child.root, stdout: out, stderr: err, timeout: 10s))?
  assert status.exited_with(1)
}

# origin: gnu chmod/octal.log
test test_gnu_chmod_octal_log { |ctx|
  let s = uu.scene(ctx)?
  for mode in ["0-anything", "7-anything", "8"] {
    uu.fails_with_code(uu.invoke(s, "chmod", [mode, "."])?, 1)
  }
}

# origin: gnu chmod/only-op.log
test test_gnu_chmod_only_op_log { |ctx|
  let s = uu.scene(ctx)?
  assert fs.stat(p"/")?.uid == 0
  for operation in ["+", "-", "="] {
    let r = uu.invoke(s, "chmod", [operation, "/"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "chmod: changing permissions of '/'")
  }
}

# origin: gnu chmod/partial-fail.log
test test_gnu_chmod_partial_fail_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  let r = uu.invoke(s, "chmod", ["0", "missing_file", "file"])?
  uu.fails_with_code(r, 1)
  uu.touch(s, "unreadable")?
  uu.set_mode(s, "unreadable", 0)?
  assert uu.mode(s, "file")? == 0
}

# origin: gnu chmod/setgid.log
test test_gnu_chmod_setgid_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.set_mode(s, "d", 0o755)?
  uu.succeeds(uu.invoke(s, "chmod", ["g+s", "d"], umask: 0)?)
  assert uu.mode(s, "d")? == 0o2755
  for mode in ["+", "-", "g-s", "00755", "000755", "=755", "-2000", "-7022", "755", "0755", "+2000", "-5022", "=7777,-5022"] {
    uu.succeeds(uu.invoke(s, "chmod", [mode, "d"], umask: 0)?)
    let clears = mode in ["g-s", "00755", "000755", "=755", "-2000", "-7022"]
    assert uu.mode(s, "d")? == (if clears { 0o755 } else { 0o2755 }), mode
    uu.succeeds(uu.invoke(s, "chmod", ["=2755", "d"], umask: 0)?)
  }
}

# origin: gnu chmod/silent.log
test test_gnu_chmod_silent_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [
    {util: "chmod", args: ["-f", "0", "no-such"]},
    {util: "chgrp", args: ["-f", "0", "no-such"]},
    {util: "chown", args: ["-f", "0:0", "no-such"]},
  ] {
    let r = uu.invoke(s, row.util, row.args)?
    uu.fails(r)
    uu.no_stderr(r)
  }
}

# origin: gnu chmod/symlinks.log
test test_gnu_chmod_symlinks_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.mkdir(s, "a/c")?
  uu.touch(s, "a/b/file")?
  uu.touch(s, "a/c/file")?
  uu.symlink(s, "foo", "a/dangle")?
  uu.symlink(s, "../b/file", "a/c/link")?
  uu.symlink(s, "b", "a/dirlink")?
  for row in [
    {args: ["755", "-R", "a/c"], inspected: ["a/c", "a/c/file", "a/b/file"], count: 2},
    {args: ["755", "-LR", "a/c"], inspected: ["a/c", "a/c/file", "a/b/file"], count: 3},
    {args: ["755", "-RP", "a/c/"], inspected: ["a/b/file"], count: 0},
    {args: ["755", "--dereference", "a/c/link"], inspected: ["a/b/file"], count: 1},
    {args: ["755", "--no-dereference", "a/c/link"], inspected: ["a/b/file"], count: 0},
  ] {
    uu.succeeds(uu.invoke(s, "chmod", ["=777", "a/b", "a/c", "a/b/file", "a/c/file"])?)
    uu.succeeds(uu.invoke(s, "chmod", row.args)?)
    var count = 0
    for name in row.inspected { if uu.mode(s, name)? == 0o755 { count += 1 } }
    assert count == row.count
  }
  for option in ["-h", "-RP", "-P"] {
    uu.succeeds(uu.invoke(s, "chmod", ["755", "--no-dereference", option, "a/dangle"])?)
  }
  for options in [[], ["--deref"], ["-R"]] {
    uu.fails_with_code(uu.invoke(s, "chmod", ["755"].extend(options).extend(["a/dangle"]))?, 1)
  }
  uu.mkdir(s, "cyc/b/c")?
  uu.symlink(s, uu.at(s, "cyc").display(), "cyc/b/c/d")?
  let cycle = uu.invoke(s, "chmod", ["-vRL", "+r", "cyc"], timeout: 10s)?
  uu.succeeds(cycle)
  let combined = bytes.concat([cycle.stdout, cycle.stderr]).utf8()?
  assert "'cyc/b/c/d'" in combined
  assert "'cyc/b/c/d/b'" not in combined
}

# origin: gnu chmod/thru-dangling.log
test test_gnu_chmod_thru_dangling_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "non-existent", "dangle")?
  let r = uu.invoke(s, "chmod", ["644", "dangle"])?
  uu.fails(r)
  uu.stderr_is(r, "chmod: cannot operate on dangling symlink 'dangle'\n")
}

# origin: gnu chmod/umask-x.log
test test_gnu_chmod_umask_x_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.set_mode(s, "file", 0o755)?
  uu.fails_with_code(uu.invoke(s, "chmod", ["-x", "file"], umask: 0o077)?, 1)
}

# origin: gnu chmod/usage.log
test test_gnu_chmod_usage_log { |ctx|
  let s = uu.scene(ctx)?
  let matrix = [
    {args: ["--"], files: []},
    {args: ["--", "--"], files: []},
    {args: ["--", "--", "--", "f"], files: ["--", "f"]},
    {args: ["--", "--", "-w", "f"], files: ["-w", "f"]},
    {args: ["--", "--", "f"], files: ["f"]},
    {args: ["--", "-w"], files: []},
    {args: ["--", "-w", "--", "f"], files: ["--", "f"]},
    {args: ["--", "-w", "-w", "f"], files: ["-w", "f"]},
    {args: ["--", "-w", "f"], files: ["f"]},
    {args: ["--", "f"], files: []},
    {args: ["-w"], files: []},
    {args: ["-w", "--"], files: []},
    {args: ["-w", "--", "--", "f"], files: ["--", "f"]},
    {args: ["-w", "--", "-w", "f"], files: ["-w", "f"]},
    {args: ["-w", "--", "f"], files: ["f"]},
    {args: ["-w", "-w"], files: []},
    {args: ["-w", "-w", "--", "f"], files: ["f"]},
    {args: ["-w", "-w", "-w", "f"], files: ["f"]},
    {args: ["-w", "-w", "f"], files: ["f"]},
    {args: ["-w", "f"], files: ["f"]},
    {args: ["f"], files: []},
    {args: ["f", "--"], files: []},
    {args: ["f", "-w"], files: ["f"]},
    {args: ["f", "f"], files: []},
    {args: ["u+gr", "f"], files: []},
    {args: ["ug,+x", "f"], files: []},
  ]
  let all_files = ["--", "-w", "f"]
  for row in matrix {
    if row.files.len() == 0 {
      for name in all_files { uu.touch(s, name)? }
      uu.fails_with_code(uu.invoke(s, "chmod", row.args)?, 1)
    } else {
      for name in row.files { uu.touch(s, name)? }
      uu.succeeds(uu.invoke(s, "chmod", row.args)?)
      for missing in row.files {
        for name in all_files { uu.remove(s, name)? }
        for name in row.files { uu.touch(s, name)? }
        uu.remove(s, missing)?
        uu.fails_with_code(uu.invoke(s, "chmod", row.args)?, 1)
      }
    }
  }
}
