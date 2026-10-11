use support.uu

proc installation(s: uu.Scene, args: List[Str], output: Str) [fs, process, env, error] {
  let r = uu.invoke(s, "install", args)?
  uu.succeeds(r)
  uu.stdout_is(r, output)
}

# Descriptor and signal setup stays outside the applet launcher, so the oracle
# receives exactly the same inherited process state and lossless argument words.
proc shell_install(s: uu.Scene, args: List[Str], setup: Str) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let launch = uu.argv(s, "install", [Path(word) for word in args])?
  let argv = [p"/bin/sh", p"-c", Path(setup), p"install-state"].extend(launch)
  let out = uu.at(s, ".setup-out")
  let err = uu.at(s, ".setup-err")
  let status = process.run(process.command_argv(p"/bin/sh", argv, s.root, {}, b"", out, err, timeout: 30s))?
  Ok({util: "install", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# origin: gnu install/create-leading.log
test test_gnu_install_create_leading_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file", "foo\n")?
  uu.succeeds(uu.invoke(s, "install", ["-D", "file", "no-dir1/no-dir2/dest"])?)
  assert uu.dir_exists(s, "no-dir1/no-dir2")?
  assert uu.file_exists(s, "no-dir1/no-dir2/dest")?
  uu.mkdir(s, "dir1")?
  uu.touch(s, "dir1/file1")?
  uu.succeeds(uu.invoke(s, "install", ["-D", uu.at(s, "dir1/file1").display(), "file", "-t", f"{s.root}/no-dir2/"])?)
  assert uu.file_exists(s, "no-dir2/file")?
  assert uu.file_exists(s, "no-dir2/file1")?
}

# origin: gnu install/d-slashdot.log
test test_gnu_install_d_slashdot_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["d1/.", "d2/.."] { uu.succeeds(uu.invoke(s, "install", ["-d", name])?) }
  for name in ["d1", "d2"] { assert uu.dir_exists(s, name)? }
}

# origin: gnu install/install-C.log
test test_gnu_install_install_C_log { |ctx|
  let s = uu.scene(ctx)?
  assert fs.stat(s.root)?.mode.bit_and(0o2000) == 0
  assert fs.stat(s.root)?.gid == group.current()?.gid
  uu.write(s, "a", "test\n")?
  let first = "'a' -> 'b'\n"
  let replaced = "removed 'b'\n'a' -> 'b'\n"
  installation(s, ["-Cv", "-m0644", "a", "b"], first)
  installation(s, ["-Cv", "-m0644", "a", "b"], "")
  installation(s, ["-v", "--compare", "-m0644", "a", "b"], "")
  installation(s, ["-v", "-m0644", "a", "b"], replaced)
  for round in range(2) { installation(s, ["-Cv", "-m2755", "a", "b"], replaced) }
  installation(s, ["-v", "-m0644", "a", "b"], replaced)
  installation(s, ["-v", "-m0644", "a", "d"], "'a' -> 'd'\n")
  uu.symlink(s, "a", "c")?
  installation(s, ["-Cv", "-m0644", "c", "d"], "")
  uu.remove(s, "d")?
  uu.symlink(s, "b", "d")?
  installation(s, ["-Cv", "-m0644", "c", "d"], "removed 'd'\n'c' -> 'd'\n")
  for data in ["test1\n", "test2\n"] {
    uu.write(s, "a", data)?
    installation(s, ["-Cv", "-m0644", "a", "b"], replaced)
    installation(s, ["-Cv", "-m0644", "a", "b"], "")
  }
  installation(s, ["-Cv", "-m0755", "a", "b"], replaced)
  installation(s, ["-Cv", "-m0755", "a", "b"], "")
  for name in ["a", "b"] { uu.write(s, name, "a\n")? }
  let dated = 1767225600000000000
  fs.set_times(uu.at(s, "a"), atime_ns: dated, mtime_ns: dated)?
  assert fs.stat(uu.at(s, "b"))?.mtime_ns > fs.stat(uu.at(s, "a"))?.mtime_ns
  uu.succeeds(uu.invoke(s, "install", ["-C", "a", "b"])?)
  assert fs.stat(uu.at(s, "b"))?.mtime_ns > fs.stat(uu.at(s, "a"))?.mtime_ns
  uu.succeeds(uu.invoke(s, "install", ["-C", "--preserve-timestamps", "a", "b"])?)
  assert time.format(fs.stat(uu.at(s, "b"))?.mtime_ns, "%F", utc: true)? == "2026-01-01"
  uu.write(s, "b", "b\n")?
  uu.succeeds(uu.invoke(s, "install", ["-C", "a", "b"])?)
  assert fs.stat(uu.at(s, "b"))?.mtime_ns > fs.stat(uu.at(s, "a"))?.mtime_ns
  uu.succeeds(uu.invoke(s, "install", ["-C", "--preserve-timestamps", "a", "b"])?)
  assert time.format(fs.stat(uu.at(s, "b"))?.mtime_ns, "%F", utc: true)? == "2026-01-01"
  uu.fails_with_code(uu.invoke(s, "install", ["-C", "--strip", "--strip-program=echo", "a", "b"])?, 1)
}

# origin: gnu install/stdin.log
test test_gnu_install_stdin_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\n")?
  uu.write(s, "b", "b\n")?
  let new_pipe = shell_install(s, ["/dev/stdin", "file1"], r"""cat a | "$@"; """)?
  uu.succeeds(new_pipe)
  uu.no_output(new_pipe)
  assert uu.read(s, "file1")? == uu.read(s, "a")?
  let new_file = uu.invoke_from_path(s, "install", ["/dev/stdin", "file2"], uu.at(s, "b"))?
  uu.succeeds(new_file)
  uu.no_output(new_file)
  assert uu.read(s, "file2")? == uu.read(s, "b")?
  let existing_file = uu.invoke_from_path(s, "install", ["/dev/stdin", "file1"], uu.at(s, "file2"))?
  uu.succeeds(existing_file)
  uu.no_output(existing_file)
  assert uu.read(s, "file1")? == uu.read(s, "b")?
  let existing_pipe = shell_install(s, ["/dev/stdin", "file1"], r"""cat b | "$@"; """)?
  uu.succeeds(existing_pipe)
  uu.no_output(existing_pipe)
  assert uu.read(s, "file1")? == uu.read(s, "b")?
}

# origin: gnu install/strip-program.log
test test_gnu_install_strip_program_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "src", "abc\n")?
  # This child transforms the payload and replaces the file supplied by install.
  uu.write(s, "b", "#!/bin/sh\ntr 'b' 'B' < \"$1\" > \"$1.t\" && /bin/mv \"$1.t\" \"$1\"\n")?
  uu.set_mode(s, "b", 0o755)?
  uu.succeeds(uu.invoke(s, "install", ["src", "dest", "-s", "--strip-program=./b"])?)
  uu.file_is(s, "dest", "aBc\n")
  uu.fails_with_code(uu.invoke(s, "install", ["src", "dest2", "-s", "--strip-program=./FOO"])?, 1)
  assert ! uu.exists(s, "dest2")?
  uu.write(s, "c", "#!/bin/sh\nkill -TERM \"$$\"\n")?
  uu.set_mode(s, "c", 0o755)?
  uu.fails_with_code(uu.invoke(s, "install", ["src", "dest3", "-s", "--strip-program=./c"])?, 1)
  assert ! uu.exists(s, "dest3")?
  uu.write(s, "no-hyphen", "#!/bin/sh\ncase \"$1\" in -*) exit 1;; esac\nprintf '%s\\n' \"$1\"\n")?
  uu.set_mode(s, "no-hyphen", 0o755)?
  uu.succeeds(uu.invoke(s, "install", ["-s", "--strip-program=./no-hyphen", "--", "src", "-dest"])?)
}

# origin: gnu install/trap.log
test test_gnu_install_trap_log { |ctx|
  let s = uu.scene(ctx)?
  # A built executable supplies the ELF input; its symbols and size do not
  # affect the child-wait invariant under an inherited ignored SIGCHLD.
  let r = shell_install(s, ["-s", s.ctx.xsh_bin.display(), "."], r"""trap '' CHLD; exec "$@"; """)?
  uu.succeeds(r)
}
