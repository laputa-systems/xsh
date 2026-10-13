use support.uu

pure shell_words(words: List[Path]) -> Str {
  ["'" + word.display().replace("'", with: "'\\''") + "'" for word in words].join(" ")
}

proc terminal_run(s: uu.Scene, util: Str, args: List[Path], redirect = "", umask: Int? = null) -> Result[Int] {
  let words = uu.argv(s, util, args, umask:)?
  let command = shell_words(words) + redirect
  let plan = process.command_argv(p"/usr/bin/script", ["script", "-q", "-e", "-c", command, "/dev/null"], s.root,
    stdout: uu.at(s, "terminal-output"), stderr: uu.at(s, "terminal-error"), timeout: 5s)
  Ok(process.run(plan)?.exit_code()?)
}

# origin: gnu misc/arch.log
test test_gnu_misc_arch_log { |ctx|
  let s = uu.scene(ctx)?
  let expected = uu.invoke(s, "uname", ["-m"])?
  uu.succeeds(expected)
  for args in [[], ["--"]] { let r = uu.invoke(s, "arch", args)?; uu.succeeds(r); assert r.stdout == expected.stdout }
}

# origin: gnu misc/basename.log
test test_gnu_misc_basename_log { |ctx|
  let s = uu.scene(ctx)?
  { let r = uu.invoke(s, "basename", ["d/f"])?; uu.succeeds(r); uu.stdout_only(r, "f\n") }
  { let r = uu.invoke(s, "basename", ["/d/f"])?; uu.succeeds(r); uu.stdout_only(r, "f\n") }
  { let r = uu.invoke(s, "basename", ["d/f/"])?; uu.succeeds(r); uu.stdout_only(r, "f\n") }
  { let r = uu.invoke(s, "basename", ["d/f//"])?; uu.succeeds(r); uu.stdout_only(r, "f\n") }
  { let r = uu.invoke(s, "basename", ["f"])?; uu.succeeds(r); uu.stdout_only(r, "f\n") }
  { let r = uu.invoke(s, "basename", ["/"])?; uu.succeeds(r); uu.stdout_only(r, "/\n") }
  { let r = uu.invoke(s, "basename", ["//"])?; uu.succeeds(r); uu.stdout_only(r, "/\n") }
  { let r = uu.invoke(s, "basename", ["///"])?; uu.succeeds(r); uu.stdout_only(r, "/\n") }
  { let r = uu.invoke(s, "basename", ["///a///"])?; uu.succeeds(r); uu.stdout_only(r, "a\n") }
  { let r = uu.invoke(s, "basename", [""])?; uu.succeeds(r); uu.stdout_only(r, "\n") }
  { let r = uu.invoke(s, "basename", ["aa", "a"])?; uu.succeeds(r); uu.stdout_only(r, "a\n") }
  { let r = uu.invoke(s, "basename", ["-a", "a", "b"])?; uu.succeeds(r); uu.stdout_only(r, "a\nb\n") }
  { let r = uu.invoke(s, "basename", ["-s", "a", "aa", "ba", "ab"])?; uu.succeeds(r); uu.stdout_only(r, "a\nb\nab\n") }
  { let r = uu.invoke(s, "basename", ["a-a", "-a"])?; uu.succeeds(r); uu.stdout_only(r, "a\n") }
  { let r = uu.invoke(s, "basename", ["f.s", ".s"])?; uu.succeeds(r); uu.stdout_only(r, "f\n") }
  { let r = uu.invoke(s, "basename", ["fs", "s"])?; uu.succeeds(r); uu.stdout_only(r, "f\n") }
  { let r = uu.invoke(s, "basename", ["fs", "fs"])?; uu.succeeds(r); uu.stdout_only(r, "fs\n") }
  { let r = uu.invoke(s, "basename", ["fs/", "s"])?; uu.succeeds(r); uu.stdout_only(r, "f\n") }
  { let r = uu.invoke(s, "basename", ["dir/file.suf", ".suf"])?; uu.succeeds(r); uu.stdout_only(r, "file\n") }
  { let r = uu.invoke(s, "basename", ["//", "/"])?; uu.succeeds(r); uu.stdout_only(r, "/\n") }
  { let r = uu.invoke(s, "basename", ["//", "//"])?; uu.succeeds(r); uu.stdout_only(r, "/\n") }
  { let r = uu.invoke(s, "basename", ["fs", "x"])?; uu.succeeds(r); uu.stdout_only(r, "fs\n") }
  { let r = uu.invoke(s, "basename", ["fs", ""])?; uu.succeeds(r); uu.stdout_only(r, "fs\n") }
  { let r = uu.invoke(s, "basename", ["fs/", "s/"])?; uu.succeeds(r); uu.stdout_only(r, "fs\n") }
  { let r = uu.invoke(s, "basename", [])?; uu.fails_with_code(r, 1); uu.stderr_only(r, "basename: missing operand\nTry 'basename --help' for more information.\n") }
  { let r = uu.invoke(s, "basename", ["a", "b", "c"])?; uu.fails_with_code(r, 1); uu.stderr_only(r, "basename: extra operand 'c'\nTry 'basename --help' for more information.\n") }
  { let r = uu.invoke(s, "basename", ["-z", "a"])?; uu.succeeds(r); uu.stdout_only(r, "a\x00") }
  { let r = uu.invoke(s, "basename", ["--zero", "a"])?; uu.succeeds(r); uu.stdout_only(r, "a\x00") }
  { let r = uu.invoke(s, "basename", ["-za", "a", "b"])?; uu.succeeds(r); uu.stdout_only(r, "a\x00b\x00") }
  { let r = uu.invoke(s, "basename", ["-z", "ba", "a"])?; uu.succeeds(r); uu.stdout_only(r, "b\x00") }
  { let r = uu.invoke(s, "basename", ["-z", "-s", "a", "ba"])?; uu.succeeds(r); uu.stdout_only(r, "b\x00") }
  for style in ["literal", "shell-always", "invalid"] { uu.stdout_only(uu.invoke(s, "basename", ["d/q name"], vars: {QUOTING_STYLE: style})?, "q name\n") }
  uu.stdout_only(uu.invoke(s, "basename", ["-z", "q name"], vars: {QUOTING_STYLE: "invalid"})?, "q name\0")
}

# origin: gnu misc/dirname.log
test test_gnu_misc_dirname_log { |ctx|
  let s = uu.scene(ctx)?
  { let r = uu.invoke(s, "dirname", ["d/f"])?; uu.succeeds(r); uu.stdout_only(r, "d\n") }
  { let r = uu.invoke(s, "dirname", ["/d/f"])?; uu.succeeds(r); uu.stdout_only(r, "/d\n") }
  { let r = uu.invoke(s, "dirname", ["d/f/"])?; uu.succeeds(r); uu.stdout_only(r, "d\n") }
  { let r = uu.invoke(s, "dirname", ["d/f//"])?; uu.succeeds(r); uu.stdout_only(r, "d\n") }
  { let r = uu.invoke(s, "dirname", ["f"])?; uu.succeeds(r); uu.stdout_only(r, ".\n") }
  { let r = uu.invoke(s, "dirname", ["/"])?; uu.succeeds(r); uu.stdout_only(r, "/\n") }
  { let r = uu.invoke(s, "dirname", ["//"])?; uu.succeeds(r); uu.stdout_only(r, "/\n") }
  { let r = uu.invoke(s, "dirname", ["///"])?; uu.succeeds(r); uu.stdout_only(r, "/\n") }
  { let r = uu.invoke(s, "dirname", ["//a//"])?; uu.succeeds(r); uu.stdout_only(r, "/\n") }
  { let r = uu.invoke(s, "dirname", ["///a///"])?; uu.succeeds(r); uu.stdout_only(r, "/\n") }
  { let r = uu.invoke(s, "dirname", ["///a///b"])?; uu.succeeds(r); uu.stdout_only(r, "///a\n") }
  { let r = uu.invoke(s, "dirname", ["///a//b/"])?; uu.succeeds(r); uu.stdout_only(r, "///a\n") }
  { let r = uu.invoke(s, "dirname", [""])?; uu.succeeds(r); uu.stdout_only(r, ".\n") }
  { let r = uu.invoke(s, "dirname", ["a/b", "c/d"])?; uu.succeeds(r); uu.stdout_only(r, "a\nc\n") }
  { let r = uu.invoke(s, "dirname", ["-z", "a/b"])?; uu.succeeds(r); uu.stdout_only(r, "a\x00") }
  { let r = uu.invoke(s, "dirname", ["--zero", "a/b"])?; uu.succeeds(r); uu.stdout_only(r, "a\x00") }
  { let r = uu.invoke(s, "dirname", ["-z", "a/b", "b/a"])?; uu.succeeds(r); uu.stdout_only(r, "a\x00b\x00") }
  let missing = uu.invoke(s, "dirname", [])?
  uu.fails_with_code(missing, 1)
  uu.stderr_only(missing, "dirname: missing operand\nTry 'dirname --help' for more information.\n")
  for style in ["literal", "shell-always", "invalid"] { uu.stdout_only(uu.invoke(s, "dirname", ["q name/f"], vars: {QUOTING_STYLE: style})?, "q name\n") }
  uu.stdout_only(uu.invoke(s, "dirname", ["-z", "q name/f"], vars: {QUOTING_STYLE: "invalid"})?, "q name\0")
}

# origin: gnu misc/echo.log
test test_gnu_misc_echo_log { |ctx|
  let s = uu.scene(ctx)?
  { let r = uu.invoke(s, "echo", ["-n", "-e", "\\x1b\\n\\e\\n\\33\\n\\033\\n\\0033\\n"])?; uu.succeeds(r); uu.stdout_only(r, "\x1b\n\x1b\n\x1b\n\x1b\n\x1b\n") }
  { let r = uu.invoke(s, "echo", ["-n", "-e", "\\x\\n"])?; uu.succeeds(r); uu.stdout_only(r, "\\x\n") }
  { let r = uu.invoke(s, "echo", ["--", "foo"])?; uu.succeeds(r); uu.stdout_only(r, "-- foo\n") }
  { let r = uu.invoke(s, "echo", ["-n", "-e", "--", "foo\\n"])?; uu.succeeds(r); uu.stdout_only(r, "-- foo\n") }
  { let r = uu.invoke(s, "echo", ["-e", "foo\\n\\cbar"])?; uu.succeeds(r); uu.stdout_only(r, "foo\n") }
  { let r = uu.invoke(s, "echo", ["-"])?; uu.succeeds(r); uu.stdout_only(r, "-\n") }
  { let r = uu.invoke(s, "echo", ["-n", "-e", "\\\\"])?; uu.succeeds(r); uu.stdout_only(r, "\\") }
  { let r = uu.invoke(s, "echo", [])?; uu.succeeds(r); uu.stdout_only(r, "\n") }
  { let r = uu.invoke(s, "echo", ["-n", "-e", "\\a\\x08\\e\\x0c\\n\\r\\t\\v"])?; uu.succeeds(r); uu.stdout_only(r, "\x07\x08\x1b\x0c\n\r\t\x0b") }
  { let r = uu.invoke(s, "echo", ["-n", "-e", "\\x4a\\x4b\\x4c\\x4d\\x4e\\x4f\\x4A\\x4B\\x4C\\x4D\\x4E\\x4F"])?; uu.succeeds(r); uu.stdout_only(r, "JKLMNOJKLMNO") }
  { let r = uu.invoke(s, "echo", [])?; uu.succeeds(r); uu.stdout_only(r, "\n") }
  { let r = uu.invoke(s, "echo", ["-n", "-E", "foo\\n"], vars: {POSIXLY_CORRECT: "1"})?; uu.succeeds(r); uu.stdout_only(r, "foo\n") }
  { let r = uu.invoke(s, "echo", ["-nE", "foo"], vars: {POSIXLY_CORRECT: "1"})?; uu.succeeds(r); uu.stdout_only(r, "-nE foo\n") }
  { let r = uu.invoke(s, "echo", ["-E", "-n", "foo"], vars: {POSIXLY_CORRECT: "1"})?; uu.succeeds(r); uu.stdout_only(r, "-E -n foo\n") }
  { let r = uu.invoke(s, "echo", ["--version"], vars: {POSIXLY_CORRECT: "1"})?; uu.succeeds(r); uu.stdout_only(r, "--version\n") }
}

# origin: gnu misc/false-status.log
test test_gnu_misc_false_status_log { |ctx|
  let s = uu.scene(ctx)?
  for option in ["--version", "--help"] {
    uu.fails_with_code(uu.invoke(s, "false", [option], stdout: p"/dev/null")?, 1)
    uu.fails_with_code(uu.invoke(s, "true", [option], stdout: p"/dev/full")?, 1)
  }
}

# origin: gnu misc/pathchk.log
test test_gnu_misc_pathchk_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")
  for args in [["file/x"], ["-P", "--", "-"], ["-p", "-P", "x/-"]] { uu.fails_with_code(uu.invoke(s, "pathchk", args)?, 1) }
  for options in [["-p"], ["-P"], ["-p", "-P"], ["--portability"]] { let r = uu.invoke(s, "pathchk", options.extend([""]))?; uu.fails_with_code(r, 1); uu.stderr_only(r, "pathchk: empty file name\n") }
}

# origin: gnu misc/mknod.log
test test_gnu_misc_mknod_log { |ctx|
  let s = uu.scene(ctx)?
  for entry in [{util: "mknod", name: "f1"}, {util: "mkfifo", name: "f2"}, {util: "mkdir", name: "f3"}] {
    let args = ["-m", "734", entry.name].extend(if entry.util == "mknod" { ["p"] } else { [] })
    uu.succeeds(uu.invoke(s, entry.util, args, umask: 0o777)?)
    let mode = uu.mode(s, entry.name)?
    assert mode == 0o734 or (entry.util == "mkdir" and mode == 0o2734)
    assert fs.stat(uu.at(s, entry.name))?.kind == (if entry.util == "mkdir" { "dir" } else { "fifo" })
  }
  for entry in [{util: "mknod", name: "f4"}, {util: "mkfifo", name: "f5"}, {util: "mkdir", name: "f6"}] {
    let args = ["--mode=ug+rw,o+r", entry.name].extend(if entry.util == "mknod" { ["p"] } else { [] })
    uu.succeeds(uu.invoke(s, entry.util, args, umask: 0o777)?)
    assert uu.mode(s, entry.name)? == (if entry.util == "mkdir" { 0o777 } else { 0o666 })
    assert fs.stat(uu.at(s, entry.name))?.kind == (if entry.util == "mkdir" { "dir" } else { "fifo" })
  }
}

# origin: gnu misc/xstrtol.log
test test_gnu_misc_xstrtol_log { |ctx|
  let s = uu.scene(ctx)?
  let large = ["9" for _ in range(81)].join("")
  for value in [large + "h", large, "x", "9x"] {
    let r = uu.invoke(s, "pr", ["--pages=" + value])?
    uu.fails_with_code(r, 1)
    let diagnostic = if value == large + "h" { f"pr: invalid suffix in --pages argument '{value}'\n" } else if value == large { f"pr: --pages argument '{value}' too large\n" } else if value == "9x" { "pr: invalid page range '9x'\n" } else { f"pr: invalid --pages argument '{value}'\n" }
    uu.stderr_only(r, diagnostic)
  }
}

# origin: gnu misc/uname-labeled.log
test test_gnu_misc_uname_labeled_log { |ctx|
  let s = uu.scene(ctx)?
  let short = uu.invoke(s, "uname", ["-A"])?
  let long = uu.invoke(s, "uname", ["--all-labeled"])?
  uu.succeeds(short); uu.succeeds(long)
  assert short.stdout == long.stdout
  let lines = short.stdout.utf8()?.split("\n")
  for label in ["Kernel name: ", "Node name: ", "Kernel release: ", "Kernel version: ", "Machine: ", "Operating system: "] { assert lines |> any { |line| line.starts_with(label) } }
  let standard = uu.invoke(s, "uname", ["-a"])?
  uu.succeeds(standard)
  let text = standard.stdout as Str
  assert text.split("\n").len() == 2
  for label in ["Kernel name:", "Node name:", "Kernel release:"] { assert label not in text }
}

# origin: gnu misc/dircolors.log
test test_gnu_misc_dircolors_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "k", "exec\n")
  { let r = uu.invoke(s, "dircolors", ["-b", "k"])?; uu.fails_with_code(r, 1); uu.stderr_only(r, "dircolors: k:1: invalid line;  missing second token\n") }
  uu.write(s, "config", "exec 'echo Hello;:'\n")
  { let r = uu.invoke(s, "dircolors", ["-b", "config"], vars: {TERM: "none"})?; uu.succeeds(r); uu.stdout_only(r, "LS_COLORS='ex='\\''echo Hello;\\:'\\'':';\nexport LS_COLORS\n") }
  uu.write(s, "config", "owt 40;33\n")
  { let r = uu.invoke(s, "dircolors", ["-b", "config"], vars: {TERM: "none"})?; uu.succeeds(r); uu.stdout_only(r, "LS_COLORS='tw=40;33:';\nexport LS_COLORS\n") }
  uu.write(s, "config", "TERM none\nowt 40;33\n")
  { let r = uu.invoke(s, "dircolors", ["-b", "config"], vars: {TERM: "none"})?; uu.succeeds(r); uu.stdout_only(r, "LS_COLORS='tw=40;33:';\nexport LS_COLORS\n") }
  uu.write(s, "config", "TERM non*\nowt 40;33\n")
  { let r = uu.invoke(s, "dircolors", ["-b", "config"], vars: {TERM: "none"})?; uu.succeeds(r); uu.stdout_only(r, "LS_COLORS='tw=40;33:';\nexport LS_COLORS\n") }
  uu.write(s, "config", "TERM nomatch\nowt 40;33\n")
  { let r = uu.invoke(s, "dircolors", ["-b", "config"], vars: {TERM: "none"})?; uu.succeeds(r); uu.stdout_only(r, "LS_COLORS='';\nexport LS_COLORS\n") }
  uu.write(s, "config", "TERM N*match\nowt 40;33\n")
  { let r = uu.invoke(s, "dircolors", ["-b", "config"], vars: {TERM: "none"})?; uu.succeeds(r); uu.stdout_only(r, "LS_COLORS='';\nexport LS_COLORS\n") }
  uu.write(s, "config", "TERM []\nowt 40;33\n")
  { let r = uu.invoke(s, "dircolors", ["-b", "config"], vars: {TERM: "none"})?; uu.succeeds(r); uu.stdout_only(r, "LS_COLORS='';\nexport LS_COLORS\n") }
  uu.write(s, "config", "COLORTERM ?*\nowt 40;33\n")
  { let r = uu.invoke(s, "dircolors", ["-b", "config"], vars: {COLORTERM: "any", TERM: "none"})?; uu.succeeds(r); uu.stdout_only(r, "LS_COLORS='tw=40;33:';\nexport LS_COLORS\n") }
  uu.write(s, "config", "COLORTERM ?*\nowt 40;33\n")
  { let r = uu.invoke(s, "dircolors", ["-b", "config"], vars: {COLORTERM: "", TERM: "none"})?; uu.succeeds(r); uu.stdout_only(r, "LS_COLORS='';\nexport LS_COLORS\n") }
  { let r = uu.invoke(s, "dircolors", ["-p", "--print-ls"])?; uu.fails_with_code(r, 1); uu.stdout_is(r, ""); uu.stderr_is(r, "dircolors: options --print-database and --print-ls-colors are mutually exclusive\nTry 'dircolors --help' for more information.\n") }
  { let r = uu.invoke(s, "dircolors", ["-b", "--print-database"])?; uu.fails_with_code(r, 1); uu.stdout_is(r, ""); uu.stderr_is(r, "dircolors: the options to output non shell syntax,\nand to select a shell syntax are mutually exclusive\nTry 'dircolors --help' for more information.\n") }
  uu.write(s, "config", "OWT 40;33\n")
  { let r = uu.invoke(s, "dircolors", ["--print-ls-colors", "config"])?; uu.succeeds(r); uu.stdout_is(r, "\x1b[40;33mtw\t40;33\x1b[0m\n"); uu.stderr_is(r, "") }
}

# origin: gnu misc/tsort.log
test test_gnu_misc_tsort_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "t b\nt s\ns t\n")
  { let r = uu.invoke(s, "tsort", ["f"])?; uu.fails_with_code(r, 1); uu.stdout_is(r, "s\nt\nb\n"); uu.stderr_is(r, "tsort: f: input contains a loop:\ntsort: s\ntsort: t\n") }
  uu.write(s, "f", "t x\nt s\ns t\n")
  { let r = uu.invoke(s, "tsort", ["f"])?; uu.fails_with_code(r, 1); uu.stdout_is(r, "s\nt\nx\n"); uu.stderr_is(r, "tsort: f: input contains a loop:\ntsort: s\ntsort: t\n") }
  uu.write(s, "f", "a a\na b\na c\nc a\nb a")
  { let r = uu.invoke(s, "tsort", ["-w", "f"])?; uu.fails_with_code(r, 1); uu.stdout_is(r, "a\nc\nb\n"); uu.stderr_is(r, "tsort: f: input contains a loop:\ntsort: a\ntsort: b\ntsort: f: input contains a loop:\ntsort: a\ntsort: c\n") }
  uu.write(s, "posix-1.1", "a b c c d e\ng g\nf g e f\nh h\n")
  { let r = uu.invoke(s, "tsort", ["posix-1.1"])?; uu.succeeds(r); uu.stdout_is(r, "a\nc\nd\nh\nb\ne\nf\ng\n"); uu.stderr_is(r, "") }
  uu.write(s, "posix-2.1", "b a\nd c\nz h x h r h\n")
  { let r = uu.invoke(s, "tsort", ["posix-2.1"])?; uu.succeeds(r); uu.stdout_is(r, "b\nd\nr\nx\nz\na\nc\nh\n"); uu.stderr_is(r, "") }
  uu.write(s, "linear-1.1", "a b b c c d d e e f f g\n")
  { let r = uu.invoke(s, "tsort", ["linear-1.1"])?; uu.succeeds(r); uu.stdout_is(r, "a\nb\nc\nd\ne\nf\ng\n"); uu.stderr_is(r, "") }
  uu.write(s, "tree-1.1", "a b b c c d d e e f f g\nc x x y y z\n")
  { let r = uu.invoke(s, "tsort", ["tree-1.1"])?; uu.succeeds(r); uu.stdout_is(r, "a\nb\nc\nx\nd\ny\ne\nz\nf\ng\n"); uu.stderr_is(r, "") }
  uu.write(s, "tree-2.1", "a b b c c d d e e f f g\nc x x y y z\nf r r s s t\n")
  { let r = uu.invoke(s, "tsort", ["tree-2.1"])?; uu.succeeds(r); uu.stdout_is(r, "a\nb\nc\nx\nd\ny\ne\nz\nf\nr\ng\ns\nt\n"); uu.stderr_is(r, "") }
  uu.write(s, "odd.1", "a\n")
  { let r = uu.invoke(s, "tsort", ["odd.1"])?; uu.fails_with_code(r, 1); uu.stdout_is(r, ""); uu.stderr_is(r, "tsort: odd.1: input contains an odd number of tokens\n") }
  uu.touch(s, "f"); uu.touch(s, "g")
  { let r = uu.invoke(s, "tsort", ["f", "g"])?; uu.fails_with_code(r, 1); uu.stdout_is(r, ""); uu.stderr_is(r, "tsort: extra operand 'g'\nTry 'tsort --help' for more information.\n") }
}

# origin: gnu misc/nohup.log
test test_gnu_misc_nohup_log { |ctx|
  let s = uu.scene(ctx)?
  let child = r"""printf 'stdout\n'; printf 'stderr\n' >&2; """
  repeat 2 times { let r = uu.invoke(s, "nohup", ["sh", "-c", child])?; uu.succeeds(r); uu.stdout_is(r, "stdout\n"); uu.stderr_is(r, "stderr\n"); assert ! uu.exists(s, "nohup.out")? }
  let _ = uu.invoke(s, "nohup", ["no-such-command"])?
  uu.touch(s, "k"); uu.set_mode(s, "k", 0)
  uu.fails_with_code(uu.invoke(s, "nohup", ["./k"])?, 126)
  uu.fails_with_code(uu.invoke(s, "nohup", [])?, 125)
  uu.fails_with_code(uu.invoke(s, "nohup", [], vars: {POSIXLY_CORRECT: "1"})?, 127)
  for input in ["", " </dev/null"] {
    assert terminal_run(s, "nohup", [p"sh", p"-c", fp"{child}"], input + " 2>err")? == 0
    uu.file_is(s, "nohup.out", "stdout\n")
    let advisory = if input == "" { "nohup: ignoring input and appending output to 'nohup.out'\n" } else { "nohup: appending output to 'nohup.out'\n" }
    uu.file_is(s, "err", advisory + "stderr\n")
    uu.remove(s, "nohup.out")
  }
  assert terminal_run(s, "nohup", [p"sh", p"-c", fp"{child}"], " >out")? == 0
  uu.file_is(s, "out", "stdout\nstderr\n")
  assert ! uu.exists(s, "nohup.out")?
  assert terminal_run(s, "nohup", [p"echo", p"hi"], " 2>/dev/full")? == 125
  uu.file_is(s, "nohup.out", "")
  uu.remove(s, "nohup.out")
  assert terminal_run(s, "nohup", [p"no-such-command"], " 2>err")? == 127
  uu.file_is(s, "nohup.out", "")
  uu.remove(s, "nohup.out")
  assert terminal_run(s, "nohup", [p"./k"], " 2>err")? == 126
  uu.file_is(s, "nohup.out", "")
  uu.remove(s, "nohup.out")
  assert terminal_run(s, "nohup", [p"echo", p"hi"], umask: 0o600)? == 0
  assert uu.mode(s, "nohup.out")? == 0o600
  uu.remove(s, "nohup.out")

}

# origin: gnu misc/sync.log
test test_gnu_misc_sync_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")
  for args in [["--data", "--file-system"], ["-d"]] { uu.fails_with_code(uu.invoke(s, "sync", args)?, 1) }
  uu.succeeds(uu.invoke(s, "sync", ["file"])?)
  uu.set_mode(s, "file", 0o200)
  uu.succeeds(uu.invoke(s, "sync", ["file"])?)
  uu.fails_with_code(uu.invoke(s, "sync", ["file", "nofile"])?, 1)
  let r = uu.invoke(s, "sync", ["nofile1", "file", "nofile2"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "sync: error opening 'nofile1': No such file or directory\nsync: error opening 'nofile2': No such file or directory\n")
  uu.mkdir(s, "norw"); uu.set_mode(s, "norw", 0)
  let inaccessible = uu.invoke(s, "sync", ["norw"])?
  uu.fails_with_code(inaccessible, 1)
  uu.stderr_is(inaccessible, "sync: error opening 'norw': Permission denied\n")
  uu.mkfifo(s, "fifo")
  for args in [["fifo"], ["-d", "fifo"]] { assert uu.invoke(s, "sync", args, timeout: 10s) is Ok(_) }
  if env.get("RUN_VERY_EXPENSIVE_TESTS") is Ok("yes") { assert uu.invoke(s, "sync", ["-f", "fifo"], timeout: 60s) is Ok(_) }
}

# origin: gnu misc/realpath.log
test test_gnu_misc_realpath_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1/dir2")
  uu.symlink(s, "dir1/dir2", "ldir2")
  uu.touch(s, "dir1/f"); uu.touch(s, "dir1/dir2/f")
  for entry in [{name: "one", target: "/"}, {name: "two", target: "//"}, {name: "three", target: "///"}] { uu.symlink(s, entry.target, entry.name) }
  uu.succeeds(uu.invoke(s, "realpath", ["-Pqz", "."])?)
  for args in [[], ["--relative-base", ".", "--relative-to", "."], ["--relative-base", "."], ["-e", "--relative-to=dir1/f", "--relative-base=.", "."], ["-m", ""], ["--relative-base=", "--relative-to=.", "."]] { uu.fails_with_code(uu.invoke(s, "realpath", args)?, 1) }
  for args in [["-e", "--relative-to=dir1/", "--relative-base=.", "."], ["-e", "-E", "--relative-to=dir1/f", "--relative-base=.", "."]] { uu.succeeds(uu.invoke(s, "realpath", args)?) }
  for style in ["literal", "shell-always", "invalid"] { uu.stdout_only(uu.invoke(s, "realpath", ["-m", "--relative-to=.", "q name"], vars: {QUOTING_STYLE: style})?, "q name\n") }
  uu.stdout_only(uu.invoke(s, "realpath", ["-zm", "--relative-to=.", "q name"], vars: {QUOTING_STYLE: "invalid"})?, "q name\0")
  let here = uu.invoke(s, "realpath", ["."])?.stdout.utf8()?.trim()
  for entry in [{args: ["ldir2/.."], tail: "/dir1"}, {args: ["-L", "ldir2/.."], tail: ""}, {args: ["-s", "ldir2"], tail: "/ldir2"}] { uu.stdout_is(uu.invoke(s, "realpath", entry.args)?, here + entry.tail + "\n") }
  { let r = uu.invoke(s, "realpath", ["-m", "--relative-to=prefix", "prefixed/1"])?; uu.succeeds(r); uu.stdout_is(r, "../prefixed/1\n") }
  { let r = uu.invoke(s, "realpath", ["-m", "--relative-to=prefixed", "prefix/1"])?; uu.succeeds(r); uu.stdout_is(r, "../prefix/1\n") }
  { let r = uu.invoke(s, "realpath", ["-m", "--relative-to=prefixed", "prefixed/1"])?; uu.succeeds(r); uu.stdout_is(r, "1\n") }
  { let r = uu.invoke(s, "realpath", ["-sm", "--relative-to=/usr", "/"])?; uu.succeeds(r); uu.stdout_is(r, "..\n") }
  { let r = uu.invoke(s, "realpath", ["-sm", "--relative-to=/", "/usr"])?; uu.succeeds(r); uu.stdout_is(r, "usr\n") }
  { let r = uu.invoke(s, "realpath", ["-sm", "--relative-base=/usr", "--relative-to=/usr", "/tmp", "/usr"])?; uu.succeeds(r); uu.stdout_is(r, "/tmp\n.\n") }
  { let r = uu.invoke(s, "realpath", ["-sm", "--relative-base=/", "--relative-to=/", "/", "/usr"])?; uu.succeeds(r); uu.stdout_is(r, ".\nusr\n") }
  { let r = uu.invoke(s, "realpath", ["-sm", "--relative-base=/usr", "/tmp", "/usr"])?; uu.succeeds(r); uu.stdout_is(r, "/tmp\n.\n") }
  { let r = uu.invoke(s, "realpath", ["-sm", "--relative-base=/", "/", "/usr"])?; uu.succeeds(r); uu.stdout_is(r, ".\nusr\n") }
  { let r = uu.invoke(s, "realpath", ["-sm", "--relative-base=/usr/local", "--relative-to=/usr", "/usr", "/usr/local"])?; uu.succeeds(r); uu.stdout_is(r, "/usr\n/usr/local\n") }
  { let r = uu.invoke(s, "realpath", ["/", "//", "///"])?; uu.succeeds(r); uu.stdout_is(r, "/\n/\n/\n") }
  { let r = uu.invoke(s, "realpath", ["one", "two", "three"])?; uu.succeeds(r); uu.stdout_is(r, "/\n/\n/\n") }
  { let r = uu.invoke(s, "realpath", ["-sm", "--relative-to=/", "/", "//", "/dev", "//dev"])?; uu.succeeds(r); uu.stdout_is(r, ".\n.\ndev\ndev\n") }
  { let r = uu.invoke(s, "realpath", ["-sm", "--relative-to=//", "/", "//", "/dev", "//dev"])?; uu.succeeds(r); uu.stdout_is(r, ".\n.\ndev\ndev\n") }
  { let r = uu.invoke(s, "realpath", ["--relative-base=/", "--relative-to=//", "/", "//"])?; uu.succeeds(r); uu.stdout_is(r, ".\n.\n") }
  uu.mkdir(s, "noread"); uu.set_mode(s, "noread", 0o311)
  for args in [["noread/"], ["-e", "noread/"]] { uu.stdout_is(uu.invoke(s, "realpath", args)?, here + "/noread\n") }
}

# origin: gnu misc/sleep.log
test test_gnu_misc_sleep_log { |ctx|
  let s = uu.scene(ctx)?
  for args in [["invalid"], ["--", "-1"], ["42D"], ["42d", "42day"], ["nan"], [""], []] { uu.fails_with_code(uu.invoke(s, "sleep", args, timeout: 10s)?, 1) }
  for interval in ["0.001", "0x.002p1", "0x0.01d"] { uu.succeeds(uu.invoke(s, "sleep", [interval], timeout: 10s)?) }
  for args in [["1d", "2h", "3m", "4s"], ["inf"], ["1.189731495357231765e+4932"]] { assert uu.invoke(s, "sleep", args, timeout: 100ms) is Err(is Timeout) }
  let locale = {LC_ALL: "fr_FR.utf8"}
  let first = uu.invoke(s, "printf", ["%0.3f", "0.001"], vars: locale)?
  uu.succeeds(first)
  let second = uu.invoke(s, "printf", ["%0.3f", first.stdout as Str], vars: locale)?
  uu.succeeds(second)
  let decimal = second.stdout as Str
  assert decimal in ["0.001", "0,001"]
  let sleeper = uu.argv(s, "sleep", [fp"{decimal}"], vars: locale)?
  let args = [fp"1{decimal}"].extend(sleeper)
  let limiter = uu.argv(s, "timeout", args, vars: locale)?
  assert process.run(process.command_argv(limiter[0], limiter, s.root, locale, timeout: 5s))?.exited_with(0)

}

# origin: gnu misc/kill.log
test test_gnu_misc_kill_log { |ctx|
  let s = uu.scene(ctx)?
  for args in [[], ["-TERM"], ["-l", "-l"], ["-l", "-t"], ["-l", "-s", "1"], ["-t", "-s", "1"], ["-0", "no_pid"], ["-l", "-1"], ["-l", "-1", "0"], ["-l", "INVALID", "TERM"]] { uu.fails_with_code(uu.invoke(s, "kill", args)?, 1) }
  let pid = run.text sh -c r"printf '%s' $PPID"
  for option in ["-0", "-s0", "-n0", "-CONT", "-Cont"] { uu.succeeds(uu.invoke(s, "kill", [option, pid])?) }
  uu.fails_with_code(uu.invoke(s, "kill", ["-cont", pid])?, 1)
  uu.succeeds(uu.invoke(s, "kill", ["-0", "-1"])?)
  for args in [["-l"], ["-t"], ["-L"], ["-t", "TERM", "HUP"]] { uu.succeeds(uu.invoke(s, "kill", args)?) }
  let names = uu.invoke(s, "kill", ["-l"])?.stdout.utf8()?.trim().split("\n")
  let term = uu.invoke(s, "kill", ["-l", "HUP", "TERM"])?.stdout.utf8()?.trim().split("\n")[1]
  uu.stdout_is(uu.invoke(s, "kill", ["-l", term])?, "TERM\n")
  uu.stdout_is(uu.invoke(s, "kill", ["-l", "143", "271"])?, "TERM\nTERM\n")
  let last = uu.invoke(s, "kill", ["-l", "--", names[-1]])?.stdout.utf8()?.trim() as Int
  let numbers = [f"{i}" for i in range(last + 1)]
  for option in ["-l", "-t"] { uu.succeeds(uu.invoke(s, "kill", [option, "--"].extend(numbers))?) }
  uu.stdout_is(uu.invoke(s, "kill", ["-l", names[0]])?, "0\n")
  assert names |> any { |name| name.starts_with("RTMIN") }
  assert names |> any { |name| name.starts_with("RTMAX") }
}

# origin: gnu misc/yes.log
test test_gnu_misc_yes_log { |ctx|
  let s = uu.scene(ctx)?
  let head = uu.argv(s, "head", [p"-n", p"2"])?.require(NonEmpty[Path])?
  let plain = uu.argv(s, "yes", [])?.require(NonEmpty[Path])?
  let sampled = run.text --accept=[0, 1, 141] @plain | run @head
  assert sampled == "y\ny\n"
  for size in [1, 1999, 4095, 4096, 8191, 8192, 16383, 16384] {
    let word = [" " for _ in range(size)].join("")
    let words = uu.argv(s, "yes", [fp"{word}"])?.require(NonEmpty[Path])?
    let word_sample = run.text --accept=[0, 1, 141] @words | run @head
    assert word_sample == word + "\n" + word + "\n"
  }
  for count in [100, 4000] {
    let args = [f"{i + 1}" for i in range(count)]
    let words = uu.argv(s, "yes", [fp"{arg}" for arg in args])?.require(NonEmpty[Path])?
    let number_sample = run.text --accept=[0, 1, 141] @words | run @head
    assert number_sample == args.join(" ") + "\n" + args.join(" ") + "\n"
  }
  for size in [1, 16384] {
    let word = [" " for _ in range(size)].join("")
    let r = uu.invoke(s, "yes", [word], stdout: p"/dev/full")?
    uu.fails_with_code(r, 1)
    uu.stderr_is(r, "yes: standard output: No space left on device\n")
  }
  assert uu.invoke(s, "yes", [], stdout: p"/dev/null", timeout: 100ms) is Err(is Timeout)
  let fallback = [p"/usr/bin/strace", p"-f", p"-o", p"/dev/null", p"-e", p"inject=io_uring_setup,io_uring_enter,io_uring_register,memfd_create,sendfile,splice,tee,vmsplice:error=ENOSYS"].extend(plain).require(NonEmpty[Path])?
  let fallback_sample = run.text --accept=[0, 1, 141] @fallback | run @head
  assert fallback_sample == "y\ny\n"
  let no_pipe = [p"/usr/bin/strace", p"-f", p"-o", p"/dev/null", p"-e", p"inject=pipe,pipe2:error=EMFILE"].extend(plain)
  let blocked = process.command_argv(p"/usr/bin/strace", no_pipe, s.root, stdout: p"/dev/null", stderr: uu.at(s, "fault-error"), timeout: 100ms)
  assert process.run(blocked) is Err(is Timeout)

}

# origin: gnu misc/close-stdout.log
test test_gnu_misc_close_stdout_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")
  for entry in [
    {util: "cp", args: ["a", "b"]},
    {util: "chmod", args: ["o-w", "."]},
    {util: "ln", args: ["a", "c"]},
    {util: "rm", args: ["c"]},
    {util: "mkdir", args: ["d"]},
    {util: "mv", args: ["d", "e"]},
    {util: "rmdir", args: ["e"]},
    {util: "touch", args: ["e"]},
    {util: "sleep", args: ["0"]},
    {util: "true", args: []},
    {util: "printf", args: [""]},
  ] {
    let words = uu.argv(s, entry.util, [fp"{arg}" for arg in entry.args])?
    let wrapper = [@words[..3], p"/bin/sh", p"-c", p"exec 1>&-; exec \"$@\"; ", p"closed-output", @words[3..]]
    let status = process.run(process.command_argv(wrapper[0], wrapper, s.root, stdout: uu.at(s, "out"), stderr: uu.at(s, "err"), timeout: 5s))?
    assert status.exited_with(0), f"{entry.util}: {status.shell_code()?} {uu.read_text(s, "err")?}"
  }
  assert uu.exists(s, "b")?
  for entry in [{util: "printf", args: ["foo"]}, {util: "cp", args: ["--verbose", "a", "b"]}, {util: "mktemp", args: ["tmpfile-XXXXXX"]}, {util: "mktemp", args: ["tmpfile-XXXXXX", "-q"]}] {
    let words = uu.argv(s, entry.util, [fp"{arg}" for arg in entry.args])?
    let wrapper = [@words[..3], p"/bin/sh", p"-c", p"exec 1>&-; exec \"$@\"; ", p"closed-output", @words[3..]]
    let result = process.run(process.command_argv(wrapper[0], wrapper, s.root, stderr: uu.at(s, "err"), timeout: 5s))?
    assert result.exited_with(1), f"{entry.util} {entry.args.join(" ")}: {result.shell_code()?} {uu.read_text(s, "err")?}"
  }
  for entry in [{util: "printf", args: ["foo"]}, {util: "cp", args: ["--verbose", "a", "b"]}, {util: "mktemp", args: ["-d", "tmpdir-XXXXXX"]}, {util: "mktemp", args: ["-d", "-q", "tmpdir-XXXXXX"]}] { uu.fails_with_code(uu.invoke(s, entry.util, entry.args, stdout: p"/dev/full")?, 1) }
  assert fs.files(s.root)? |> all { |entry| ! entry.name.starts_with("tmpfile-") }
}

# origin: gnu misc/no-fork.log
test test_gnu_misc_no_fork_log { |ctx|
  let s = uu.scene(ctx)?
  # stdbuf needs an LD_PRELOAD library to configure child libc buffering; the standalone static musl build does not ship it.
  for entry in [{util: "chroot", args: ["--skip-chdir", "/", "true"]}, {util: "env", args: ["true"]}, {util: "nice", args: ["true"]}, {util: "nohup", args: ["true"]}] {
    let words = uu.argv(s, entry.util, [fp"{arg}" for arg in entry.args])?
    let wrapper = [@words[..3], p"/bin/bash", p"-c", p"ulimit -u 0 || exit; exec \"$@\"; ", p"no-fork", @words[3..]]
    let status = process.run(process.command_argv(wrapper[0], wrapper, s.root, stderr: uu.at(s, "err"), timeout: 5s))?
    if entry.util != "chroot" { assert status.exited_with(0), uu.read_text(s, "err")? }
  }
}

# origin: gnu misc/invalid-opt.log
test test_gnu_misc_invalid_opt_log { |ctx|
  let s = uu.scene(ctx)?
  for entry in [
    {util: "chroot", code: 125, output: ""},
    {util: "hostid", code: 1, output: ""},
    {util: "timeout", code: 125, output: ""},
    {util: "nice", code: 125, output: ""},
    {util: "who", code: 1, output: ""},
    {util: "users", code: 1, output: ""},
    {util: "pinky", code: 1, output: ""},
    {util: "stty", code: 1, output: ""},
    {util: "df", code: 1, output: ""},
    {util: "stdbuf", code: 125, output: ""},
    {util: "b2sum", code: 1, output: ""},
    {util: "base64", code: 1, output: ""},
    {util: "base32", code: 1, output: ""},
    {util: "basenc", code: 1, output: ""},
    {util: "basename", code: 1, output: ""},
    {util: "cat", code: 1, output: ""},
    {util: "chgrp", code: 1, output: ""},
    {util: "chmod", code: 1, output: ""},
    {util: "chown", code: 1, output: ""},
    {util: "cksum", code: 1, output: ""},
    {util: "comm", code: 1, output: ""},
    {util: "cp", code: 1, output: ""},
    {util: "csplit", code: 1, output: ""},
    {util: "cut", code: 1, output: ""},
    {util: "date", code: 1, output: ""},
    {util: "dd", code: 1, output: ""},
    {util: "dir", code: 2, output: ""},
    {util: "dircolors", code: 1, output: ""},
    {util: "dirname", code: 1, output: ""},
    {util: "du", code: 1, output: ""},
    {util: "echo", code: 0, output: "-/\n"},
    {util: "env", code: 125, output: ""},
    {util: "expand", code: 1, output: ""},
    {util: "expr", code: 0, output: "-/\n"},
    {util: "factor", code: 1, output: ""},
    {util: "false", code: 1, output: ""},
    {util: "fmt", code: 1, output: ""},
    {util: "fold", code: 1, output: ""},
    {util: "install", code: 1, output: ""},
    {util: "groups", code: 1, output: ""},
    {util: "head", code: 1, output: ""},
    {util: "id", code: 1, output: ""},
    {util: "join", code: 1, output: ""},
    {util: "link", code: 1, output: ""},
    {util: "ln", code: 1, output: ""},
    {util: "logname", code: 1, output: ""},
    {util: "ls", code: 2, output: ""},
    {util: "md5sum", code: 1, output: ""},
    {util: "mkdir", code: 1, output: ""},
    {util: "mkfifo", code: 1, output: ""},
    {util: "mknod", code: 1, output: ""},
    {util: "mktemp", code: 1, output: ""},
    {util: "mv", code: 1, output: ""},
    {util: "nl", code: 1, output: ""},
    {util: "nproc", code: 1, output: ""},
    {util: "nohup", code: 125, output: ""},
    {util: "numfmt", code: 1, output: ""},
    {util: "od", code: 1, output: ""},
    {util: "paste", code: 1, output: ""},
    {util: "pathchk", code: 1, output: ""},
    {util: "pr", code: 1, output: ""},
    {util: "printenv", code: 2, output: ""},
    {util: "printf", code: 0, output: "-/"},
    {util: "ptx", code: 1, output: ""},
    {util: "pwd", code: 1, output: ""},
    {util: "readlink", code: 1, output: ""},
    {util: "realpath", code: 1, output: ""},
    {util: "rm", code: 1, output: ""},
    {util: "rmdir", code: 1, output: ""},
    {util: "seq", code: 1, output: ""},
    {util: "sha1sum", code: 1, output: ""},
    {util: "sha224sum", code: 1, output: ""},
    {util: "sha256sum", code: 1, output: ""},
    {util: "sha384sum", code: 1, output: ""},
    {util: "sha512sum", code: 1, output: ""},
    {util: "shred", code: 1, output: ""},
    {util: "shuf", code: 1, output: ""},
    {util: "sleep", code: 1, output: ""},
    {util: "sort", code: 2, output: ""},
    {util: "split", code: 1, output: ""},
    {util: "stat", code: 1, output: ""},
    {util: "sum", code: 1, output: ""},
    {util: "sync", code: 1, output: ""},
    {util: "tac", code: 1, output: ""},
    {util: "tail", code: 1, output: ""},
    {util: "tee", code: 1, output: ""},
    {util: "test", code: 0, output: ""},
    {util: "touch", code: 1, output: ""},
    {util: "tr", code: 1, output: ""},
    {util: "true", code: 0, output: ""},
    {util: "truncate", code: 1, output: ""},
    {util: "tsort", code: 1, output: ""},
    {util: "tty", code: 2, output: ""},
    {util: "uname", code: 1, output: ""},
    {util: "unexpand", code: 1, output: ""},
    {util: "uniq", code: 1, output: ""},
    {util: "unlink", code: 1, output: ""},
    {util: "vdir", code: 2, output: ""},
    {util: "wc", code: 1, output: ""},
    {util: "whoami", code: 1, output: ""},
    {util: "yes", code: 1, output: ""},
    {util: "arch", code: 1, output: ""},
    {util: "kill", code: 1, output: ""},
    {util: "uptime", code: 1, output: ""},
    {util: "hostname", code: 1, output: ""},
  ] { let r = uu.invoke(s, entry.util, ["-/"])?; uu.fails_with_code(r, entry.code); uu.stdout_is(r, entry.output) }
}

# origin: gnu misc/option-aliases.log
test test_gnu_misc_option_aliases_log { |ctx|
  let s = uu.scene(ctx)?
  for entry in [
    {util: "head", args: ["--quiet", "--silent", "--version"]},
    {util: "chown", args: ["--quiet", "--silent", "--version"]},
    {util: "date", args: ["--rfc-email", "--rfc-822", "--rfc-2822", "--version"]},
    {util: "date", args: ["--uct", "--utc", "--universal", "--version"]},
    {util: "uname", args: ["--kernel-name", "--sysname", "--version"]},
    {util: "uname", args: ["--kernel-release", "--release", "--version"]},
  ] { uu.succeeds(uu.invoke(s, entry.util, entry.args, stdout: p"/dev/null")?) }
}
