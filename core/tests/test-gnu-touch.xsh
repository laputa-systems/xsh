use support.uu

# Descriptor setup belongs to the parent shell; the child argv remains shared
# with the oracle, including no-create operations on a closed stdout descriptor.
proc descriptor_touch(s: uu.Scene, args: List[Str], setup: Str) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let launch = uu.argv(s, "touch", [Path(arg) for arg in args])?
  let wrapper = [p"/bin/sh", p"-c", Path(setup), p"touch-descriptor"]
  let argv = if "1>&-" in setup { launch[0..3].extend(wrapper).extend(launch[3..]) } else { wrapper.extend(launch) }
  let out = uu.at(s, ".descriptor-out")
  let err = uu.at(s, ".descriptor-err")
  let status = process.run(process.command_argv(argv[0], argv, s.root, {}, b"", out, err, timeout: 10s))?
  Ok({util: "touch", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

proc date_is(s: uu.Scene, name: Str, expected: Str) [fs, time, error] {
  assert time.format(fs.stat(uu.at(s, name))?.mtime_ns, "%F", utc: true)? == expected
}

# origin: gnu touch/60-seconds.log
test test_gnu_touch_60_seconds_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "touch", ["-t", "197001010000.60", "f"], vars: {TZ: "UTC0"})?)
  assert fs.stat(uu.at(s, "f"))?.mtime_ns == 60000000000
}

# origin: gnu touch/dangling-symlink.log
test test_gnu_touch_dangling_symlink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "touch-target", "t-symlink")?
  uu.succeeds(uu.invoke(s, "touch", ["t-symlink"], timeout: 10s)?)
  assert uu.file_exists(s, "touch-target")?
  for name in ["touch-target", "t-symlink"] { uu.remove(s, name)? }
}

# origin: gnu touch/dir-1.log
test test_gnu_touch_dir_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "touch", ["."])?)
}

# origin: gnu touch/empty-file.log
test test_gnu_touch_empty_file_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "b", "c"] { uu.touch(s, name)?; assert uu.file_exists(s, name)? }
  time.sleep(2s)
  uu.succeeds(uu.invoke(s, "touch", ["./a"])?)
  assert fs.stat(uu.at(s, "a"))?.mtime_ns > fs.stat(uu.at(s, "b"))?.mtime_ns
  time.sleep(2s)
  uu.succeeds(uu.invoke(s, "touch", ["./b"])?)
  assert fs.stat(uu.at(s, "b"))?.mtime_ns > fs.stat(uu.at(s, "a"))?.mtime_ns
  let descriptor = descriptor_touch(s, ["-"], r"""exec "$@" 1<./c 2>/dev/null; """)?
  if descriptor.status == 0 {
    assert fs.stat(uu.at(s, "c"))?.mtime_ns > fs.stat(uu.at(s, "a"))?.mtime_ns
  }
  for name in ["a", "b", "c"] { uu.remove(s, name)? }
}

# origin: gnu touch/fail-diag.log
test test_gnu_touch_fail_diag_log { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["/no-such-dir/file"])?
  uu.fails(r)
  assert bytes.concat([r.stdout, r.stderr]) == b"touch: cannot touch '/no-such-dir/file': No such file or directory\n"
}

# origin: gnu touch/fifo.log
test test_gnu_touch_fifo_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  uu.succeeds(uu.invoke(s, "touch", ["fifo"], timeout: 10s)?)
}

# origin: gnu touch/no-create-missing.log
test test_gnu_touch_no_create_missing_log { |ctx|
  let s = uu.scene(ctx)?
  for flag in ["-c", "-cm", "-ca"] {
    uu.succeeds(uu.invoke(s, "touch", [flag, "no-file"])?)
    uu.succeeds(descriptor_touch(s, [flag, "-"], r"""exec "$@" 1>&- 2>/dev/null; """)?)
  }
}

# origin: gnu touch/no-dereference.log
test test_gnu_touch_no_dereference_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "nowhere", "dangling")?
  uu.touch(s, "file")?
  uu.symlink(s, "file", "link")?
  let missing = uu.invoke(s, "touch", ["-h", "no-file"])?
  uu.fails_with_code(missing, 1)
  assert ! missing.stderr.is_empty()
  let quiet = uu.invoke(s, "touch", ["-h", "-c", "no-file"])?
  uu.succeeds(quiet)
  uu.no_stderr(quiet)
  uu.succeeds(uu.invoke(s, "touch", ["-h", "file"])?)
  uu.succeeds(uu.invoke(s, "touch", ["-h", "-r", "dangling", "file"])?)
  assert ! uu.exists(s, "nowhere")?
  let dangling = uu.invoke(s, "touch", ["-h", "dangling"])?
  uu.succeeds(dangling)
  uu.no_stderr(dangling)
  assert ! uu.exists(s, "nowhere")?
  uu.succeeds(uu.invoke(s, "touch", ["-m", "-h", "-d", "2009-10-10", "link"])?)
  date_is(s, "link", "2009-10-10")
  assert time.format(fs.stat(uu.at(s, "file"))?.mtime_ns, "%F", utc: true)? != "2009-10-10"
  uu.succeeds(descriptor_touch(s, ["-h", "-"], r"""exec "$@" >file; """)?)
  uu.fails_with_code(descriptor_touch(s, ["-h", "-"], r"""exec "$@" 1>&-; """)?, 1)
  uu.succeeds(descriptor_touch(s, ["-h", "-c", "-"], r"""exec "$@" 1>&-; """)?)
}

# origin: gnu touch/no-rights.log
test test_gnu_touch_no_rights_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [{date: "2000-01-01 00:00", name: "t1"}, {date: "2000-01-02 00:00", name: "t2"}] {
    uu.succeeds(uu.invoke(s, "touch", ["-d", row.date, row.name])?)
  }
  assert fs.stat(uu.at(s, "t2"))?.mtime_ns > fs.stat(uu.at(s, "t1"))?.mtime_ns
  uu.set_mode(s, "t1", 0)?
  uu.succeeds(uu.invoke(s, "touch", ["-d", "2000-01-03 00:00", "-c", "t1"])?)
  assert fs.stat(uu.at(s, "t1"))?.mtime_ns > fs.stat(uu.at(s, "t2"))?.mtime_ns
  uu.succeeds(uu.invoke(s, "touch", ["-a", "--no-create", "t1"])?)
}

# origin: gnu touch/not-owner.log
test test_gnu_touch_not_owner_log { |ctx|
  let s = uu.scene(ctx)?
  let root_meta = fs.stat(p"/")?
  assert root_meta.uid != user.current()?.uid and root_meta.gid != group.current()?.gid
  let r = uu.invoke(s, "touch", ["/"])?
  uu.fails(r)
  let output = bytes.concat([r.stdout, r.stderr]).utf8()?
  assert output in ["touch: setting times of '/': Permission denied\n", "touch: setting times of '/': Operation not permitted\n", "touch: setting times of '/': Read-only file system\n"]
}

# origin: gnu touch/obsolescent.log
test test_gnu_touch_obsolescent_log { |ctx|
  let s = uu.scene(ctx)?
  let vars = {_POSIX2_VERSION: "199209", POSIXLY_CORRECT: "1"}
  for ones in ["11111111", "1111111111"] {
    for args in [[ones], ["--", ones], ["01010000", ones], ["--", "01010000", ones]] {
      uu.succeeds(uu.invoke(s, "touch", args, vars: vars)?)
      assert uu.file_exists(s, ones)?
      assert ! uu.exists(s, "01010000")?
      uu.remove(s, ones)?
    }
  }
  uu.succeeds(uu.invoke(s, "touch", ["0101000000", "file"], vars: vars)?)
  assert uu.file_exists(s, "0101000000")? and uu.file_exists(s, "file")?
}

# origin: gnu touch/read-only.log
test test_gnu_touch_read_only_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "read-only")?
  uu.set_mode(s, "read-only", 0o444)?
  uu.succeeds(uu.invoke(s, "touch", ["read-only"])?)
  let r = descriptor_touch(s, ["-"], r"""exec "$@" 1<read-only 2>/dev/null; """)?
  if r.status == 0 { assert ! uu.exists(s, "-")? }
}

# origin: gnu touch/relative.log
test test_gnu_touch_relative_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "touch", ["--date=2004-01-16 12:00 +0000", "f"], vars: {TZ: "UTC0"})?)
  uu.succeeds(uu.invoke(s, "touch", ["--ref", "f", "--date=-5 days", "f"])?)
  date_is(s, "f", "2004-01-11")
}

# origin: gnu touch/trailing-slash.log
test test_gnu_touch_trailing_slash_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "nowhere", "dangling")?
  uu.symlink(s, "loop", "loop")?
  uu.touch(s, "file")?
  uu.symlink(s, "file", "link1")?
  uu.mkdir(s, "dir")?
  uu.symlink(s, "dir", "link2")?
  for name in ["no-file/", "file/", "dangling/", "loop/"] {
    uu.fails_with_code(uu.invoke(s, "touch", [name])?, 1)
  }
  uu.fails_with_code(uu.invoke(s, "ls", ["link1/"])?, 2)
  uu.fails_with_code(uu.invoke(s, "touch", ["link1/"])?, 1)
  uu.succeeds(uu.invoke(s, "touch", ["dir/"])?)
  for name in ["no-file/", "dangling/", "dir/"] { uu.succeeds(uu.invoke(s, "touch", ["-c", name])?) }
  for name in ["file/", "loop/"] { uu.fails_with_code(uu.invoke(s, "touch", ["-c", name])?, 1) }
  uu.fails_with_code(uu.invoke(s, "ls", ["link1/"])?, 2)
  uu.fails_with_code(uu.invoke(s, "touch", ["-c", "link1/"])?, 1)
  assert ! uu.exists(s, "no-file")? and ! uu.exists(s, "nowhere")?
  uu.succeeds(uu.invoke(s, "touch", ["-d", "2009-10-10", "-h", "link2/"])?)
  uu.succeeds(uu.invoke(s, "touch", ["-h", "-r", "link2/", "file"])?)
  date_is(s, "dir", "2009-10-10")
  assert time.format(fs.stat(uu.at(s, "link2"))?.mtime_ns, "%F", utc: true)? != "2009-10-10"
  date_is(s, "file", "2009-10-10")
}
