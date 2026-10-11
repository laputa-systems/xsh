##! Transcribed from the uutils ls integration tests.

use support.uu as uu

# Capture files belong outside the listed directory, including hidden-file listings.
proc ls_invoke(s: uu.Scene, args: List[Str], vars: Record = {}, stdout: Path? = null) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let capture = uu.scene(s.ctx)?
  let output = stdout ?? uu.at(capture, "stdout")
  let errors = uu.at(capture, "stderr")
  let r = uu.invoke(s, "ls", args, vars: vars, stdout: output, stderr: errors)?
  Ok({util: "ls", args: args, status: r.status,
    stdout: if stdout == null { output.read_bytes()? } else { b"" }, stderr: errors.read_bytes()?})
}

proc stdout_matches(r: uu.Ran, expression: Str) [error] {
  assert regex.compile(expression)?.matches(r.stdout.utf8()?), f"ls {r.args.join(" ")}: {r.stdout.utf8()?} does not match {expression}"
}

proc stdout_contains_line(r: uu.Ran, expected: Str) [error] {
  assert expected in r.stdout.utf8()?.split("\n")
}

# Set the descriptor limit in the child before the applet starts, retaining the oracle argv.
proc invoke_limited(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let words = uu.argv(s, "ls", [Path(word) for word in args])?
  let argv = [p"/bin/sh", p"-c", p"ulimit -n 20 || exit; exec \"$@\"", p"ls-limit"].extend(words)
  let output = uu.at(s, "limited-out")
  let errors = uu.at(s, "limited-err")
  let plan = process.command_argv(p"/bin/sh", argv, s.root, {}, b"", output, errors, timeout: 5s)
  let status = process.run(plan)?
  Ok({util: "ls", args: args, status: status.exit_code()?, stdout: output.read_bytes()?, stderr: errors.read_bytes()?})
}

# origin: uutils test_ls::test_ls_recursive
test test_uu_ls_ls_recursive { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "a/b", "a/b/c", "z"] { uu.mkdir(s, name)? }
  uu.touch(s, "a/a")?
  uu.touch(s, "a/b/b")?
  uu.succeeds(ls_invoke(s, ["a"])?)
  uu.succeeds(ls_invoke(s, ["a/a"])?)
  let empty = ls_invoke(s, ["z", "-R"])?
  uu.succeeds(empty)
  uu.stdout_contains(empty, "z:")
  let recursive = ls_invoke(s, ["--color=never", "-R", "a", "z"])?
  uu.succeeds(recursive)
  uu.stdout_contains(recursive, "a/b:\nb")
}

# origin: uutils test_ls::test_ls_recursive_1
test test_uu_ls_ls_recursive_1 { |ctx|
  let s = uu.scene(ctx)?
  for name in ["x", "y", "a", "b", "c", "a/1", "a/2", "a/3"] { uu.mkdir(s, name)? }
  uu.touch(s, "f")?
  uu.touch(s, "a/1/I")?
  uu.touch(s, "a/1/II")?
  let r = ls_invoke(s, ["-R1", "a", "b", "c"])?
  uu.succeeds(r)
  uu.stdout_is(r, "a:\n1\n2\n3\n\na/1:\nI\nII\n\na/2:\n\na/3:\n\nb:\n\nc:\n")
}

# origin: uutils test_ls::test_ls_recursive_all_with_version_sort_does_not_walk_up
test test_uu_ls_ls_recursive_all_with_version_sort_does_not_walk_up { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "a/b", "a/b/c"] { uu.mkdir(s, name)? }
  let r = ls_invoke(s, ["-aRv", "a/b"])?
  uu.succeeds(r)
  uu.stdout_is(r, "a/b:\n.\n..\nc\n\na/b/c:\n.\n..\n")
}

# origin: uutils test_ls::test_ls_recursive_no_fd_leak
test test_uu_ls_ls_recursive_no_fd_leak { |ctx|
  let s = uu.scene(ctx)?
  let deep = [f"{i}" for i in range(1, 31)].join("/")
  uu.mkdir_all(s, deep)?
  let r = invoke_limited(s, ["-R", "1"])?
  uu.succeeds(r)
  uu.no_stderr(r)
}

# origin: uutils test_ls::test_ls_sort_extension
test test_uu_ls_ls_sort_extension { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file1")?
  uu.touch(s, "file2")?
  uu.touch(s, "anotherFile")?
  uu.touch(s, ".hidden")?
  uu.touch(s, ".file.1")?
  uu.touch(s, ".file.2")?
  uu.touch(s, "file.1")?
  uu.touch(s, "file.2")?
  uu.touch(s, "anotherFile.1")?
  uu.touch(s, "anotherFile.2")?
  uu.touch(s, "file.ext")?
  uu.touch(s, "file.debug")?
  uu.touch(s, "anotherFile.ext")?
  uu.touch(s, "anotherFile.debug")?
  let r0 = ls_invoke(s, ["-1aX"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "anotherFile\nfile1\nfile2\n.\n..\n.file.1\nanotherFile.1\nfile.1\n.file.2\nanotherFile.2\nfile.2\nanotherFile.debug\nfile.debug\nanotherFile.ext\nfile.ext\n.hidden\n")
  let r1 = ls_invoke(s, ["-1a", "--sort=extension"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "anotherFile\nfile1\nfile2\n.\n..\n.file.1\nanotherFile.1\nfile.1\n.file.2\nanotherFile.2\nfile.2\nanotherFile.debug\nfile.debug\nanotherFile.ext\nfile.ext\n.hidden\n")
}

# origin: uutils test_ls::test_ls_version_sort
test test_uu_ls_ls_version_sort { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a2")?
  uu.touch(s, "b1")?
  uu.touch(s, "b20")?
  uu.touch(s, "a1.4")?
  uu.touch(s, "a1.40")?
  uu.touch(s, "b3")?
  uu.touch(s, "b11")?
  uu.touch(s, "b20b")?
  uu.touch(s, "b20a")?
  uu.touch(s, "a100")?
  uu.touch(s, "a1.13")?
  uu.touch(s, "aa")?
  uu.touch(s, "a1")?
  uu.touch(s, "aaa")?
  uu.touch(s, "a1.00000040")?
  uu.touch(s, "abab")?
  uu.touch(s, "ab")?
  uu.touch(s, "a01.40")?
  uu.touch(s, "a001.001")?
  uu.touch(s, "a01.0000001")?
  uu.touch(s, "a01.001")?
  uu.touch(s, "a001.01")?
  let r0 = ls_invoke(s, ["-1v"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "a1\na001.001\na001.01\na01.0000001\na01.001\na1.4\na1.13\na01.40\na1.00000040\na1.40\na2\na100\naa\naaa\nab\nabab\nb1\nb3\nb11\nb20\nb20a\nb20b\n")
  let r1 = ls_invoke(s, ["-1", "--sort=version"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a1\na001.001\na001.01\na01.0000001\na01.001\na1.4\na1.13\na01.40\na1.00000040\na1.40\na2\na100\naa\naaa\nab\nabab\nb1\nb3\nb11\nb20\nb20a\nb20b\n")
  let r2 = ls_invoke(s, ["-1", "--sort=v"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "a1\na001.001\na001.01\na01.0000001\na01.001\na1.4\na1.13\na01.40\na1.00000040\na1.40\na2\na100\naa\naaa\nab\nabab\nb1\nb3\nb11\nb20\nb20a\nb20b\n")
  let r3 = ls_invoke(s, ["-a1v"])?
  uu.succeeds(r3)
  uu.stdout_is(r3, ".\n..\na1\na001.001\na001.01\na01.0000001\na01.001\na1.4\na1.13\na01.40\na1.00000040\na1.40\na2\na100\naa\naaa\nab\nabab\nb1\nb3\nb11\nb20\nb20a\nb20b\n")
}

# origin: uutils test_ls::test_ls_version_sort_command_line_args
test test_uu_ls_ls_version_sort_command_line_args { |ctx|
  let s = uu.scene(ctx)?
  for name in ["10", "18", "9.5"] {
    uu.mkdir(s, name)?
    uu.touch(s, f"{name}/file")?
  }
  let r = ls_invoke(s, ["-1v", "10/file", "18/file", "9.5/file"])?
  uu.succeeds(r)
  uu.stdout_only(r, "9.5/file\n10/file\n18/file\n")
}

# origin: uutils test_ls::test_ls_sort_none
test test_uu_ls_ls_sort_none { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test-3")?
  uu.touch(s, "test-1")?
  uu.touch(s, "test-2")?
  for option in ["--sort=none", "--sort=non", "--sort=no", "-U"] { uu.succeeds(ls_invoke(s, [option])?) }
}

# origin: uutils test_ls::test_ls_sort_name
test test_uu_ls_ls_sort_name { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test-3")?
  uu.touch(s, "test-1")?
  uu.touch(s, "test-2")?
  for option in ["--sort=name", "--sort=nam", "--sort=na"] {
    let r = ls_invoke(s, [option])?
    uu.succeeds(r)
    uu.stdout_is(r, "test-1\ntest-2\ntest-3\n")
  }
  let dots = uu.scene(ctx)?
  for name in [".a", "a", ".b", "b"] { uu.touch(dots, name)? }
  let r = ls_invoke(dots, ["--sort=name", "-A"])?
  uu.succeeds(r)
  uu.stdout_is(r, ".a\n.b\na\nb\n")
}

# origin: uutils test_ls::test_ls_sort_width
test test_uu_ls_ls_sort_width { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "aaaaa")?
  uu.touch(s, "bbb")?
  uu.touch(s, "cccc")?
  uu.touch(s, "eee")?
  uu.touch(s, "d")?
  uu.touch(s, "fffff")?
  uu.touch(s, "abc")?
  uu.touch(s, "zz")?
  uu.touch(s, "bcdef")?
  for option in ["--sort=width", "--sort=widt", "--sort=w"] {
    let r = ls_invoke(s, [option])?
    uu.succeeds(r)
    uu.stdout_is(r, "d\nzz\nabc\nbbb\neee\ncccc\naaaaa\nbcdef\nfffff\n")
  }
}

# origin: uutils test_ls::test_ls_walk_glob
test test_uu_ls_ls_walk_glob { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, ".test-1")?
  uu.mkdir(s, "some-dir")?
  uu.touch(s, "some-dir/test-2~")?
  let r = ls_invoke(s, ["-1", "--ignore-backups", "some-dir"])?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  assert !("test-2~" in output)
  assert !(".." in output)
  assert !regex.compile(r"^\.\n")?.matches(output)
}

# origin: uutils test_ls::test_ls_subdired_complex
test test_uu_ls_ls_subdired_complex { |ctx|
  let s = uu.scene(ctx)?
  for name in ["dir1", "dir1/d", "dir1/c2"] { uu.mkdir(s, name)? }
  for name in ["dir1/a1", "dir1/a22", "dir1/a333", "dir1/c2/a4444"] { uu.touch(s, name)? }
  let r = ls_invoke(s, ["--dired", "-l", "-R", "dir1"])?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  let lines = [line for line in output.split("\n") if line.starts_with("//SUBDIRED//")]
  assert lines.len() > 0
  let positions = [word.parse_int()? for word in lines[0].split(" ")[1..] if word != ""]
  assert positions.len() % 2 == 0
  let names = [r.stdout[positions[i * 2]..positions[i * 2 + 1]].utf8()? for i in range(positions.len() / 2)]
  assert names == ["dir1", "dir1/c2", "dir1/d"]
}

# origin: uutils test_ls::test_ls_symlink_to_dir_with_mi_colors
test test_uu_ls_ls_symlink_to_dir_with_mi_colors { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "target_dir")?
  uu.symlink(s, "target_dir", "link")?
  let r = ls_invoke(s, ["-lp", "--color=always", "link"], vars: {LS_COLORS: "mi=41:ln=1;36:di=1;34"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "\x1b[0m\x1b[1;36mlink\x1b[0m -> \x1b[1;34mtarget_dir\x1b[0m/")
}

# origin: uutils test_ls::test_symlink_chain_target_color
test test_uu_ls_symlink_chain_target_color { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.symlink(s, "file", "link1")?
  uu.symlink(s, "link1", "link2")?
  let r = ls_invoke(s, ["-l", "--color=always", "link2"])?
  uu.succeeds(r)
  let target = r.stdout.utf8()?.split("->")[1]
  assert !("36m" in target)
}

# origin: uutils test_ls::test_symlink_target_extension_color
test test_uu_ls_symlink_target_extension_color { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "archive.tar.gz")?
  uu.symlink(s, "archive.tar.gz", "link")?
  let r = ls_invoke(s, ["-l", "--color=always", "link"], vars: {LS_COLORS: "*.tar.gz=31:or=33"})?
  uu.succeeds(r)
  let target = r.stdout.utf8()?.split("->")[1]
  assert "31m" in target
}

# origin: uutils test_ls::test_ls_width
test test_uu_ls_ls_width { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test-width-1")?
  uu.touch(s, "test-width-2")?
  uu.touch(s, "test-width-3")?
  uu.touch(s, "test-width-4")?
  for option in ["-w 100", "-w=100", "--width=100", "--width 100", "--wid=100"] {
    let r = ls_invoke(s, option.split(" ").extend(["-C"]))?
    if option.starts_with("-w=") {
      uu.fails_with_code(r, 2)
      uu.stderr_only(r, f"ls: invalid line width: '={option.byte_slice(3)}'\n")
    } else {
      uu.succeeds(r)
      uu.stdout_only(r, "test-width-1  test-width-2  test-width-3  test-width-4\n")
    }
  }
  for option in ["-w 50", "-w=50", "--width=50", "--width 50", "--wid=50"] {
    let r = ls_invoke(s, option.split(" ").extend(["-C"]))?
    if option.starts_with("-w=") {
      uu.fails_with_code(r, 2)
      uu.stderr_only(r, f"ls: invalid line width: '={option.byte_slice(3)}'\n")
    } else {
      uu.succeeds(r)
      uu.stdout_only(r, "test-width-1  test-width-3\ntest-width-2  test-width-4\n")
    }
  }
  for option in ["-w 25", "-w=25", "--width=25", "--width 25", "--wid=25"] {
    let r = ls_invoke(s, option.split(" ").extend(["-C"]))?
    if option.starts_with("-w=") {
      uu.fails_with_code(r, 2)
      uu.stderr_only(r, f"ls: invalid line width: '={option.byte_slice(3)}'\n")
    } else {
      uu.succeeds(r)
      uu.stdout_only(r, "test-width-1\ntest-width-2\ntest-width-3\ntest-width-4\n")
    }
  }
  for option in ["-w 0", "-w=0", "--width=0", "--width 0", "--wid=0"] {
    let r = ls_invoke(s, option.split(" ").extend(["-C"]))?
    if option.starts_with("-w=") {
      uu.fails_with_code(r, 2)
      uu.stderr_only(r, f"ls: invalid line width: '={option.byte_slice(3)}'\n")
    } else {
      uu.succeeds(r)
      uu.stdout_only(r, "test-width-1  test-width-2  test-width-3  test-width-4\n")
    }
  }
  for option in ["-w 062", "-w=062", "--width=062", "--width 062", "--wid=062"] {
    let r = ls_invoke(s, option.split(" ").extend(["-C"]))?
    if option.starts_with("-w=") {
      uu.fails_with_code(r, 2)
      uu.stderr_only(r, f"ls: invalid line width: '={option.byte_slice(3)}'\n")
    } else {
      uu.succeeds(r)
      uu.stdout_only(r, "test-width-1  test-width-3\ntest-width-2  test-width-4\n")
    }
  }
  for option in ["-w 100000000000000", "-w=100000000000000", "--width=100000000000000", "--width 100000000000000", "-w 07777777777777777777", "-w=07777777777777777777", "--width=07777777777777777777", "--width 07777777777777777777"] {
    let r = ls_invoke(s, option.split(" ").extend(["-C"]))?
    if option.starts_with("-w=") {
      uu.fails_with_code(r, 2)
      uu.stderr_only(r, f"ls: invalid line width: '={option.byte_slice(3)}'\n")
    } else {
      uu.succeeds(r)
      uu.stdout_only(r, "test-width-1  test-width-2  test-width-3  test-width-4\n")
    }
  }
  let bad = ls_invoke(s, ["-w=bad", "-C"])?
  uu.fails(bad)
  uu.stderr_contains(bad, "invalid line width")
  for option in ["-w 1a", "-w=1a", "--width=1a", "--width 1a", "--wid 1a"] {
    let r = ls_invoke(s, option.split(" ").extend(["-C"]))?
    uu.fails(r)
    uu.stderr_only(r, if option.starts_with("-w=") {
      "ls: invalid line width: '=1a'\n"
    } else {
      "ls: invalid line width: '1a'\n"
    })
  }
}

# origin: uutils test_ls::test_ls_zero
test test_uu_ls_ls_zero { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "0-test-zero")?
  uu.touch(s, "2-test-zero")?
  uu.touch(s, "3-test-zero")?
  let plain0 = ls_invoke(s, ["--zero"])?
  uu.succeeds(plain0)
  uu.stdout_only(plain0, "0-test-zero\x002-test-zero\x003-test-zero\x00")
  for option in ["--quoting-style=c", "--color=always", "-m", "--hide-control-chars"] {
    let r = ls_invoke(s, [option, "--zero"])?
    uu.succeeds(r)
    uu.stdout_only(r, "0-test-zero\x002-test-zero\x003-test-zero\x00")
  }
  let quoted0 = ls_invoke(s, ["--zero", "--quoting-style=c"])?
  uu.succeeds(quoted0)
  uu.stdout_only(quoted0, "\"0-test-zero\"\x00\"2-test-zero\"\x00\"3-test-zero\"\x00")
  let color0 = ls_invoke(s, ["--zero", "--color=always"])?
  uu.succeeds(color0)
  uu.stdout_is(color0, "0-test-zero\x002-test-zero\x003-test-zero\x00")
  let commas0 = ls_invoke(s, ["--zero", "-m"])?
  uu.succeeds(commas0)
  uu.stdout_only(commas0, "0-test-zero, 2-test-zero, 3-test-zero\x00")
  let controls0 = ls_invoke(s, ["--zero", "--hide-control-chars"])?
  uu.succeeds(controls0)
  uu.stdout_only(controls0, "0-test-zero\x002-test-zero\x003-test-zero\x00")
  let last_zero = ls_invoke(s, ["--zero", "--quoting-style=c", "--zero"])?
  uu.succeeds(last_zero)
  uu.stdout_only(last_zero, "0-test-zero\x002-test-zero\x003-test-zero\x00")
  uu.touch(s, "1\ntest-zero")?
  let plain1 = ls_invoke(s, ["--zero"])?
  uu.succeeds(plain1)
  uu.stdout_only(plain1, "0-test-zero\x001\ntest-zero\x002-test-zero\x003-test-zero\x00")
  for option in ["--quoting-style=c", "--color=always", "--color=alway", "--color=al", "-m", "--hide-control-chars"] {
    let r = ls_invoke(s, [option, "--zero"])?
    uu.succeeds(r)
    uu.stdout_only(r, "0-test-zero\x001\ntest-zero\x002-test-zero\x003-test-zero\x00")
  }
  let quoted1 = ls_invoke(s, ["--zero", "--quoting-style=c"])?
  uu.succeeds(quoted1)
  uu.stdout_only(quoted1, "\"0-test-zero\"\x00\"1\\ntest-zero\"\x00\"2-test-zero\"\x00\"3-test-zero\"\x00")
  let color1 = ls_invoke(s, ["--zero", "--color=always"])?
  uu.succeeds(color1)
  uu.stdout_is(color1, "0-test-zero\x001\ntest-zero\x002-test-zero\x003-test-zero\x00")
  let commas1 = ls_invoke(s, ["--zero", "-m"])?
  uu.succeeds(commas1)
  uu.stdout_only(commas1, "0-test-zero, 1\ntest-zero, 2-test-zero, 3-test-zero\x00")
  let controls1 = ls_invoke(s, ["--zero", "--hide-control-chars"])?
  uu.succeeds(controls1)
  uu.stdout_only(controls1, "0-test-zero\x001?test-zero\x002-test-zero\x003-test-zero\x00")
  let long_output = ls_invoke(s, ["-l", "--zero"])?
  uu.succeeds(long_output)
  uu.stdout_contains(long_output, "total ")
}

# origin: uutils test_ls::test_posixly_correct_and_block_size_env_vars
test test_uu_ls_posixly_correct_and_block_size_env_vars { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "dd", ["if=/dev/zero", "of=file", "bs=1024", "count=1"])?)
  let r0 = ls_invoke(s, ["-l"])?
  uu.succeeds(r0)
  stdout_contains_line(r0, "total 4")?
  uu.stdout_contains(r0, " 1024 ")
  let r1 = ls_invoke(s, ["-l"], vars: {POSIXLY_CORRECT: "some_value"})?
  uu.succeeds(r1)
  stdout_contains_line(r1, "total 8")?
  uu.stdout_contains(r1, " 1024 ")
  let r2 = ls_invoke(s, ["-l"], vars: {LS_BLOCK_SIZE: "512"})?
  uu.succeeds(r2)
  stdout_contains_line(r2, "total 8")?
  uu.stdout_contains(r2, " 2 ")
  let r3 = ls_invoke(s, ["-l"], vars: {BLOCK_SIZE: "512"})?
  uu.succeeds(r3)
  stdout_contains_line(r3, "total 8")?
  uu.stdout_contains(r3, " 2 ")
  let r4 = ls_invoke(s, ["-l"], vars: {BLOCKSIZE: "512"})?
  uu.succeeds(r4)
  stdout_contains_line(r4, "total 8")?
  uu.stdout_contains(r4, " 1024 ")
}

# origin: uutils test_ls::test_posixly_correct_and_block_size_env_vars_with_k
test test_uu_ls_posixly_correct_and_block_size_env_vars_with_k { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "dd", ["if=/dev/zero", "of=file", "bs=1024", "count=1"])?)
  let r0 = ls_invoke(s, ["-l", "-k"], vars: {POSIXLY_CORRECT: "some_value"})?
  uu.succeeds(r0)
  stdout_contains_line(r0, "total 4")?
  uu.stdout_contains(r0, " 1024 ")
  let r1 = ls_invoke(s, ["-l", "-k"], vars: {LS_BLOCK_SIZE: "512"})?
  uu.succeeds(r1)
  stdout_contains_line(r1, "total 4")?
  uu.stdout_contains(r1, " 2 ")
  let r2 = ls_invoke(s, ["-l", "-k"], vars: {BLOCK_SIZE: "512"})?
  uu.succeeds(r2)
  stdout_contains_line(r2, "total 4")?
  uu.stdout_contains(r2, " 2 ")
  let r3 = ls_invoke(s, ["-l", "-k"], vars: {BLOCKSIZE: "512"})?
  uu.succeeds(r3)
  stdout_contains_line(r3, "total 4")?
  uu.stdout_contains(r3, " 1024 ")
}

# origin: uutils test_ls::test_tabsize_option
test test_uu_ls_tabsize_option { |ctx|
  let s = uu.scene(ctx)?
  for valid in ["3", "0", "0xff"] { uu.succeeds(ls_invoke(s, ["-T", valid])?) }
  uu.succeeds(ls_invoke(s, ["--tabsize", "0"])?)
  for invalid in ["-3", "a", "3.14"] {
    let r = ls_invoke(s, [f"--tabsize={invalid}"])?
    uu.fails(r)
    uu.stderr_is(r, f"ls: invalid tab size: '{invalid}'\n")
  }
  uu.fails(ls_invoke(s, ["-T"])?)
}

# origin: uutils test_ls::test_tabsize_formatting
test test_uu_ls_tabsize_formatting { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "aaaaaaaa")?
  uu.touch(s, "bbbb")?
  uu.touch(s, "cccc")?
  uu.touch(s, "dddddddd")?
  let r0 = ls_invoke(s, ["-x", "-w18", "-T4"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "aaaaaaaa  bbbb\ncccc\t  dddddddd\n")
  let r1 = ls_invoke(s, ["-C", "-w18", "-T4"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "aaaaaaaa  cccc\nbbbb\t  dddddddd\n")
  let r2 = ls_invoke(s, ["-x", "-w18", "-T2"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "aaaaaaaa\tbbbb\ncccc\t\t\tdddddddd\n")
  let r3 = ls_invoke(s, ["-C", "-w18", "-T2"])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "aaaaaaaa\tcccc\nbbbb\t\t\tdddddddd\n")
  let r4 = ls_invoke(s, ["-x", "-w18", "-T0"])?
  uu.succeeds(r4)
  uu.stdout_is(r4, "aaaaaaaa  bbbb\ncccc      dddddddd\n")
  let r5 = ls_invoke(s, ["-C", "-w18", "-T0"])?
  uu.succeeds(r5)
  uu.stdout_is(r5, "aaaaaaaa  cccc\nbbbb      dddddddd\n")
}

# origin: uutils test_ls::test_term_colorterm
test test_uu_ls_term_colorterm { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "exe")?
  uu.succeeds(uu.invoke(s, "chmod", ["+x", "exe"])?)
  let r0 = ls_invoke(s, ["--color=always"], vars: {LS_COLORS: "", TERM: ""})?
  uu.succeeds(r0)
  assert r0.stdout.utf8()?.trim() == "exe"
  let r1 = ls_invoke(s, ["--color=always"], vars: {LS_COLORS: "", COLORTERM: ""})?
  uu.succeeds(r1)
  assert r1.stdout.utf8()?.trim() == "exe"
  let r2 = ls_invoke(s, ["--color=always"], vars: {LS_COLORS: "", TERM: "", COLORTERM: ""})?
  uu.succeeds(r2)
  assert r2.stdout.utf8()?.trim() == "exe"
  let r3 = ls_invoke(s, ["--color=always"], vars: {LS_COLORS: "", TERM: "dumb", COLORTERM: ""})?
  uu.succeeds(r3)
  assert r3.stdout.utf8()?.trim() == "exe"
}

# origin: uutils test_ls::test_suffix_case_sensitivity
test test_uu_ls_suffix_case_sensitivity { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "img1.jpg")?
  uu.touch(s, "IMG2.JPG")?
  uu.touch(s, "img3.JpG")?
  uu.touch(s, "file1.z")?
  uu.touch(s, "file2.Z")?
  let r0 = ls_invoke(s, ["-U1", "--color=always", "img1.jpg", "IMG2.JPG", "file1.z", "file2.Z"], vars: {LS_COLORS: "*.jpg=01;35:*.Z=01;31"})?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "\x1b[0m\x1b[01;35mimg1.jpg\x1b[0m\n\x1b[01;35mIMG2.JPG\x1b[0m\n\x1b[01;31mfile1.z\x1b[0m\n\x1b[01;31mfile2.Z\x1b[0m")
  let r1 = ls_invoke(s, ["-U1", "--color=always", "img1.jpg", "IMG2.JPG", "img3.JpG"], vars: {LS_COLORS: "*.jpg=01;35:*.JPG=01;35;46"})?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "\x1b[0m\x1b[01;35mimg1.jpg\x1b[0m\n\x1b[01;35;46mIMG2.JPG\x1b[0m\nimg3.JpG")
  let r2 = ls_invoke(s, ["-U1", "--color=always", "img1.jpg", "IMG2.JPG", "img3.JpG"], vars: {LS_COLORS: "*.jpg=01;35:*.JPG=01;35"})?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "\x1b[0m\x1b[01;35mimg1.jpg\x1b[0m\n\x1b[01;35mIMG2.JPG\x1b[0m\n\x1b[01;35mimg3.JpG\x1b[0m")
  let r3 = ls_invoke(s, ["-U1", "--color=always", "img1.jpg", "IMG2.JPG", "img3.JpG"], vars: {LS_COLORS: "*.jpg=01;35:*.jpg=01;35;46:*.JPG=01;35;46"})?
  uu.succeeds(r3)
  uu.stdout_contains(r3, "\x1b[0m\x1b[01;35;46mimg1.jpg\x1b[0m\n\x1b[01;35;46mIMG2.JPG\x1b[0m\n\x1b[01;35;46mimg3.JpG\x1b[0m")
  let r4 = ls_invoke(s, ["-U1", "--color=always", "img1.jpg", "IMG2.JPG", "img3.JpG"], vars: {LS_COLORS: "*.jpg=01;35;46:*.jpg=01;35:*.JPG=01;35;46"})?
  uu.succeeds(r4)
  uu.stdout_contains(r4, "\x1b[0m\x1b[01;35mimg1.jpg\x1b[0m\n\x1b[01;35;46mIMG2.JPG\x1b[0m\nimg3.JpG")
}

# origin: uutils test_ls::test_ls_time_style_env_full_iso
test test_uu_ls_ls_time_style_env_full_iso { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "t1")?
  let r = ls_invoke(s, ["-l", "t1"], vars: {TZ: "UTC", TIME_STYLE: "full-iso"})?
  uu.succeeds(r)
  stdout_matches(r, r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}")?
}

# origin: uutils test_ls::test_ls_time_style_iso_recent_and_older
test test_uu_ls_ls_time_style_iso_recent_and_older { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "recent")?
  let recent = ls_invoke(s, ["-l", "--time-style=iso", "recent"], vars: {TZ: "UTC"})?
  uu.succeeds(recent)
  stdout_matches(recent, r"(^|\n).*\d{2}-\d{2} \d{2}:\d{2} ")?
  uu.touch(s, "older")?
  fs.set_times(uu.at(s, "older"), mtime_sec: 0)?
  let older = ls_invoke(s, ["-l", "--time-style=iso", "older"])?
  uu.succeeds(older)
  stdout_matches(older, r"(^|\n).*\d{4}-\d{2}-\d{2}  +")?
}

# origin: uutils test_ls::test_ls_time_style_posix_locale_override
test test_uu_ls_ls_time_style_posix_locale_override { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "p1")?
  let r = ls_invoke(s, ["-l", "p1"], vars: {TZ: "UTC", LC_ALL: "POSIX", TIME_STYLE: "posix-full-iso"})?
  uu.succeeds(r)
  stdout_matches(r, r" [A-Z][a-z]{2} +\d{1,2} +\d{2}:\d{2} ")?
}

# origin: uutils test_ls::test_ls_time_style_precedence_last_wins
test test_uu_ls_ls_time_style_precedence_last_wins { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "timefile")?
  let first = ls_invoke(s, ["--time-style=long-iso", "--full-time", "-l", "timefile"])?
  uu.succeeds(first)
  stdout_matches(first, r"\d{2}:\d{2}:\d{2}")?
  let second = ls_invoke(s, ["--full-time", "--time-style=long-iso", "-l", "timefile"])?
  uu.succeeds(second)
  assert !regex.compile(r"\d{2}:\d{2}:\d{2}")?.matches(second.stdout.utf8()?)
}

# origin: uutils test_ls::test_ls_time_sort_without_long
test test_uu_ls_ls_time_sort_without_long { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  fs.set_times(uu.at(s, "a"), mtime_sec: 0)?
  uu.touch(s, "b")?
  let default_output = ls_invoke(s, [])?
  uu.succeeds(default_output)
  let sorted_output = ls_invoke(s, ["-t"])?
  uu.succeeds(sorted_output)
  assert default_output.stdout != sorted_output.stdout
}

# origin: uutils test_ls::test_ls_time_recent_future
test test_uu_ls_ls_time_recent_future { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test")?
  let r0 = ls_invoke(s, ["-l", "--time-style=iso"])?
  uu.succeeds(r0)
  stdout_matches(r0, "[a-z-]* \\d* [\\w.]* [\\w.]* \\d* \\d{2}-\\d{2} \\d{2}:\\d{2} test\\n")?
  fs.set_times(uu.at(s, "test"), mtime_ns: (time.now() + -8640000000) * 1000000)?
  let r1 = ls_invoke(s, ["-l", "--time-style=iso"])?
  uu.succeeds(r1)
  stdout_matches(r1, "[a-z-]* \\d* [\\w.]* [\\w.]* \\d* \\d{2}-\\d{2} \\d{2}:\\d{2} test\\n")?
  fs.set_times(uu.at(s, "test"), mtime_ns: (time.now() + -17280000000) * 1000000)?
  let r2 = ls_invoke(s, ["-l", "--time-style=iso"])?
  uu.succeeds(r2)
  stdout_matches(r2, "[a-z-]* \\d* [\\w.]* [\\w.]* \\d* \\d{4}-\\d{2}-\\d{2}  test\\n")?
  fs.set_times(uu.at(s, "test"), mtime_ns: (time.now() + 60000) * 1000000)?
  let r3 = ls_invoke(s, ["-l", "--time-style=iso"])?
  uu.succeeds(r3)
  stdout_matches(r3, "[a-z-]* \\d* [\\w.]* [\\w.]* \\d* \\d{4}-\\d{2}-\\d{2}  test\\n")?
  fs.set_times(uu.at(s, "test"), mtime_now: true)?
  let recent = ls_invoke(s, ["-l", "--time-style=+OLD\nRECENT"])?
  uu.succeeds(recent)
  uu.stdout_contains(recent, "RECENT")
  fs.set_times(uu.at(s, "test"), mtime_ns: (time.now() - 17280000000) * 1000000)?
  let old = ls_invoke(s, ["-l", "--time-style=+OLD\nRECENT"])?
  uu.succeeds(old)
  uu.stdout_contains(old, "OLD")
  let single = ls_invoke(s, ["-l", "--time-style=+RECENT"])?
  uu.succeeds(single)
  uu.stdout_contains(single, "RECENT")
}

# origin: uutils test_ls::test_time_style_timezone_name
test test_uu_ls_time_style_timezone_name { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r = ls_invoke(s, ["-l", "--time-style=+%Z"], vars: {TZ: "UTC0"})?
  uu.succeeds(r)
  stdout_matches(r, "[a-z-]* \\d* [\\w.]* [\\w.]* \\d* UTC f\\n")?
}

# origin: uutils test_ls::test_unknown_format_specifier
test test_uu_ls_unknown_format_specifier { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r = ls_invoke(s, ["-l", "--time-style=+%Y %0 %N"])?
  uu.succeeds(r)
  stdout_matches(r, "[a-z-]* \\d* [\\w.]* [\\w.]* \\d+ \\d{4} %0 \\d{9} f\\n")?
}

# origin: uutils test_ls::test_time_style_empty_after_posix_prefix
test test_uu_ls_time_style_empty_after_posix_prefix { |ctx|
  let s = uu.scene(ctx)?
  let r = ls_invoke(s, ["-l", "--time-style=posix-"])?
  uu.succeeds(r)
  uu.stdout_only(r, "total 0\n")
}

# origin: uutils test_ls::test_time_style_ambiguous_and_invalid_prefixes
test test_uu_ls_time_style_ambiguous_and_invalid_prefixes { |ctx|
  let s = uu.scene(ctx)?
  for value in ["l", "lo", "posix-l", "posix-lo", "posix-", "full-isox", "Locale"] {
    let direct = ls_invoke(s, ["-l", "--time-style", value])?
    let environment = ls_invoke(s, ["-l"], vars: {TIME_STYLE: value})?
    for r in [direct, environment] {
      if value.starts_with("posix-") {
        uu.succeeds(r)
        uu.stdout_only(r, "total 0\n")
      } else {
        uu.fails(r)
        uu.fails_with_code(r, 2)
        if value == "l" or value == "lo" {
          uu.stderr_contains(r, f"ambiguous argument '{value}' for 'time style'")
        } else {
          uu.stderr_contains(r, f"invalid argument '{value}' for 'time style'")
        }
      }
    }
  }
}

# origin: uutils test_ls::test_write_error
test test_uu_ls_write_error { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "dummy_file.txt")?
  let r = ls_invoke(s, [], stdout: p"/dev/full")?
  uu.fails_with_code(r, 2)
  uu.stderr_is(r, "ls: write error: No space left on device\n")
}
