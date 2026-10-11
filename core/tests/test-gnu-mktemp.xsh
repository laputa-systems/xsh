use support.uu

proc created(s: uu.Scene, r: uu.Ran, directory: Bool) [fs, error] -> Result[Unit, Error] {
  uu.succeeds(r)
  assert r.stdout.len() > 1 and r.stdout.ends_with(b"\n")
  let entry = uu.at_bytes(s, r.stdout[0..r.stdout.len() - 1])?
  assert fs.stat(entry)?.kind == (if directory { "dir" } else { "file" })
  Ok()
}

# Wrappers own syscall fault injection and descriptor setup, while every tested
# applet receives argv from the helper shared with the reference oracle.
proc wrapped(s: uu.Scene, args: List[Str], prefix: List[Path]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let launch = uu.argv(s, "mktemp", [Path(arg) for arg in args])?
  let words = prefix.extend(launch)
  let out = uu.at(s, ".wrapper-out")
  let err = uu.at(s, ".wrapper-err")
  let status = process.run(process.command_argv(words[0], words, s.root, {}, b"", out, err, timeout: 10s))?
  Ok({util: "mktemp", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# origin: gnu mktemp/bad-unicode.log
test test_gnu_mktemp_bad_unicode_log { |ctx|
  let s = uu.scene(ctx)?
  let bad = b"\xff|\xed\xba\xad|\xc2\x89|\xed\xa6\xbf\xed\xbf\xbf"
  uu.at_bytes(s, bad)?.write("a\n")?
  for locale in ["C", "fr_FR", "fr_FR.UTF-8"] {
    let vars = {LC_ALL: locale}
    let suffix = Path.parse_bytes(bytes.concat([b"--suffix=", bad]))?
    let file1 = uu.invoke_paths(s, "mktemp", [p"--tmpdir=.", suffix], vars: vars)?
    created(s, file1, false)?
    let dir1 = uu.invoke_paths(s, "mktemp", [p"--tmpdir=.", p"-d", suffix], vars: vars)?
    created(s, dir1, true)?
    let dir_bytes = dir1.stdout[0..dir1.stdout.len() - 1]
    let tmpdir = Path.parse_bytes(bytes.concat([b"--tmpdir=", dir_bytes]))?
    for directory in [false, true] {
      let args = if directory { [p"-d", tmpdir] } else { [tmpdir] }
      created(s, uu.invoke_paths(s, "mktemp", args, vars: vars)?, directory)?
    }
    for directory in [false, true] {
      let args = if directory { [p"-d"] } else { [] }
      created(s, uu.invoke_paths(s, "mktemp", args, vars: {LC_ALL: locale, TMPDIR: Path.parse_bytes(dir_bytes)?})?, directory)?
    }
    let pattern_word = Path.parse_bytes(bytes.concat([bad, b"XXXXXX"]))?
    for directory in [false, true] {
      let args = if directory { [p"-d", p"-t", pattern_word] } else { [p"-t", pattern_word] }
      created(s, uu.invoke_paths(s, "mktemp", args, vars: {LC_ALL: locale, TMPDIR: "."})?, directory)?
    }
  }
}

# origin: gnu mktemp/mktemp-misc.log
test test_gnu_mktemp_mktemp_misc_log { |ctx|
  let s = uu.scene(ctx)?
  let prefix = [p"setarch", p"-R"]
  let first = wrapped(s, ["-u", "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"], prefix)?
  if first.status == 0 {
    let second = wrapped(s, ["-u", "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"], prefix)?
    if second.status == 0 { assert first.stdout != second.stdout }
  }
  let probe = process.run(process.command_argv(p"strace", [p"strace", p"-o", p"/dev/null", p"-e", p"inject=getrandom:error=ENOSYS", p"true"], s.root, {}, b"", uu.at(s, "probe-out"), uu.at(s, "probe-err"), timeout: 10s))?
  if probe.exited_with(0) {
    uu.succeeds(wrapped(s, ["-u"], [p"strace", p"-o", p"/dev/null", p"-e", p"inject=getrandom:error=ENOSYS"])?)
  }
}

# origin: gnu mktemp/mktemp.log
test test_gnu_mktemp_mktemp_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [
    {args: ["-q", "a", "b"], vars: {}, error: "mktemp: too many templates\nTry 'mktemp --help' for more information.\n"},
    {args: ["-q", "foo.XX"], vars: {}, error: "mktemp: too few X's in template 'foo.XX'\n"},
    {args: ["-t", "a/bXXXX"], vars: {}, error: "mktemp: invalid template, 'a/bXXXX', contains directory separator\n"},
    {args: ["--tmpdir=a", "/bXXXX"], vars: {}, error: "mktemp: invalid template, '/bXXXX'; with --tmpdir, it may not be absolute\n"},
    {args: ["--suffix", "/b", "aXXXX"], vars: {}, error: "mktemp: invalid suffix '/b', contains directory separator\n"},
    {args: ["aXXXX/b"], vars: {}, error: "mktemp: invalid suffix '/b', contains directory separator\n"},
    {args: ["--suffix=", "aXXXXb"], vars: {}, error: "mktemp: with --suffix, template 'aXXXXb' must end in X\n"},
    {args: ["-d", "--suffix=aXXXXb", ""], vars: {}, error: "mktemp: with --suffix, template '' must end in X\n"},
    {args: ["aXXXX", "--suffix=b"], vars: {POSIXLY_CORRECT: "1"}, error: "mktemp: too many templates\nTry 'mktemp --help' for more information.\n"},
    {args: ["aXXb"], vars: {}, error: "mktemp: too few X's in template 'aXXb'\n"},
    {args: ["-d", "--suffix=X", "aXX"], vars: {}, error: "mktemp: too few X's in template 'aXX'\n"},
  ] {
    let r = uu.invoke(s, "mktemp", row.args, vars: row.vars)?
    uu.fails_with_code(r, 1)
    uu.stderr_only(r, row.error)
  }
  for row in [
    {args: ["bar.XXXX"], pattern: "^bar\\.....\n$", kind: "file", pre: "", vars: {}},
    {args: ["--", "-XXXX"], pattern: "^-....\n$", kind: "file", pre: "", vars: {}},
    {args: ["-d", "f.XXXX"], pattern: "^f\\.....\n$", kind: "dir", pre: "", vars: {}},
    {args: ["-d", "XXXX"], pattern: "^....\n$", kind: "dir", pre: "XXXX", vars: {}},
    {args: ["-u", "f.XXXX"], pattern: "^f\\.....\n$", kind: "dry", pre: "", vars: {}},
    {args: ["-d", "--dry-run", "d.XXXX"], pattern: "^d\\.....\n$", kind: "dry", pre: "", vars: {}},
    {args: ["aXXXXb"], pattern: "^a....b\n$", kind: "file", pre: "", vars: {}},
    {args: ["-d", "aXXXXb"], pattern: "^a....b\n$", kind: "dir", pre: "", vars: {}},
    {args: ["-u", "aXXXXb"], pattern: "^a....b\n$", kind: "dry", pre: "", vars: {}},
    {args: ["aXXXXaaXXXXa"], pattern: "^aXXXXaa....a\n$", kind: "file", pre: "", vars: {}},
    {args: ["-d", "--suffix=", "aXXXXaaXXXX"], pattern: "^aXXXXaa....\n$", kind: "dir", pre: "", vars: {}},
    {args: ["--suffix=b", "aXXXX"], pattern: "^a....b\n$", kind: "file", pre: "", vars: {}},
    {args: ["--suffix=X", "aXXXX"], pattern: "^a....X\n$", kind: "file", pre: "", vars: {}},
    {args: ["aXXXX", "--suffix=b"], pattern: "^a....b\n$", kind: "file", pre: "", vars: {}},
    {args: ["--suffix=.txt"], pattern: "^\\./tmp\\..{10}\\.txt\n$", kind: "file", pre: "", vars: {TMPDIR: "."}},
    {args: ["--tmpdir=.", "a/bXXXX"], pattern: "^\\./a/b....\n$", kind: "file", pre: "a", vars: {}},
    {args: ["--tmpdir=.", "-d", "a/bXXXX"], pattern: "^\\./a/b....\n$", kind: "dir", pre: "a", vars: {}},
    {args: ["--tmpdir=.", "a/.XXXX"], pattern: "^\\./a/\\.....\n$", kind: "file", pre: "a", vars: {}},
    {args: ["--tmpdir=.", "-d", "a/.XXXX"], pattern: "^\\./a/\\.....\n$", kind: "dir", pre: "a", vars: {}},
    {args: ["-t", "-p", "no/such/dir", "foo.XXX"], pattern: "^\\./foo\\....\n$", kind: "unchecked", pre: "", vars: {TMPDIR: "."}},
    {args: ["-u"], pattern: "^no/such/dir/tmp\\..{10}\n$", kind: "unchecked", pre: "", vars: {TMPDIR: "no/such/dir"}},
  ] {
    if row.pre != "" { uu.mkdir(s, row.pre)?; uu.set_mode(s, row.pre, 0o755)? }
    let r = uu.invoke(s, "mktemp", row.args, vars: row.vars)?
    uu.succeeds(r)
    uu.no_stderr(r)
    assert regex.compile(row.pattern)?.matches(r.stdout.utf8()?), f"{row.args.join(" ")}: {r.stdout.utf8()?}"
    let name = r.stdout.utf8()?.trim()
    let entry = uu.at(s, name)
    if row.kind == "dry" {
      assert ! entry.exists()?
    } else if row.kind != "unchecked" {
      assert fs.stat(entry)?.kind == row.kind
      let required = if row.kind == "dir" { 0o700 } else { 0o600 }
      assert fs.stat(entry)?.mode.bit_and(0o777) == required
      entry.remove()?
    }
    if row.pre != "" { uu.remove(s, row.pre)? }
  }
  let bad_dir = uu.invoke(s, "mktemp", [], vars: {TMPDIR: "no/such/dir"})?
  uu.fails_with_code(bad_dir, 1)
  uu.no_stdout(bad_dir)
  let normalized = regex.compile("(no/such/dir/)[^']+': .*\n")?.replace(bad_dir.stderr.utf8()?, with: "$1...\n")
  assert normalized == "mktemp: failed to create file via template 'no/such/dir/...\n"
}

# origin: gnu mktemp/write-error.log
test test_gnu_mktemp_write_error_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  for args in [["-p", "a/b"], ["-p", "a/b", "-d"]] {
    let r = wrapped(s, args, [p"/bin/sh", p"-c", p"exec \"$@\" > /dev/full", p"mktemp-output"])?
    uu.fails_with_code(r, 1)
  }
  assert (fs.children(uu.at(s, "a/b")) |> count()) == 0
}
