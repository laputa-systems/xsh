##! Transcribed from the MIT-licensed uutils ls integration tests.

use support.uu as uu

# Captures live outside the listed scene, including when ls shows hidden files.
proc invoke(s: uu.Scene, args: List[Str], vars: Record = {}, terminal: Bool = false, stderr: Path? = null) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let captures = test.temp_dir(s.ctx, name: "ls-captures")?
  let output = fp"{captures}/stdout"
  let error_output = stderr ?? fp"{captures}/stderr"
  if terminal {
    let pty = unix.open_pty()?
    defer unix.close_fd(pty.master)
    defer unix.close_fd(pty.replica)
    let plan = uu.command(s, "ls", args, vars: vars, stdout: fp"{pty.name}", stderr: error_output, timeout: 30s)?
    let status = process.run(plan)?.exit_code()?
    let data = unix.read_fd(pty.master, 65536)?
    Ok({util: "ls", args: args, status: status, stdout: data, stderr: if stderr == null { error_output.read_bytes()? } else { b"" }})
  } else {
    let plan = uu.command(s, "ls", args, vars: vars, stdout: output, stderr: error_output, timeout: 30s)?
    let status = process.run(plan)?.exit_code()?
    Ok({util: "ls", args: args, status: status, stdout: output.read_bytes()?, stderr: if stderr == null { error_output.read_bytes()? } else { b"" }})
  }
}

# A pipe-backed stdout keeps /dev/stdout metadata faithful while captures stay outside the scene.
proc invoke_pipe(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let captures = test.temp_dir(s.ctx, name: "ls-pipe-captures")?
  let out = fp"{captures}/stdout"
  let err = fp"{captures}/stderr"
  let code = fp"{captures}/status"
  let words = uu.argv(s, "ls", [Path(word) for word in args])?
  let argv = [p"/bin/sh", p"-c", Path(r"""status=$1; shift; ("$@"; rc=$?; printf '%s\n' "$rc" >"$status") | cat; """), p"ls-pipe", code].extend(words)
  let completed = process.run(process.command_argv(p"/bin/sh", argv, s.root, stdout: out, stderr: err, timeout: 30s))?
  assert completed.exited_with(0), "stdout capture pipeline failed"
  let status = code.read_text()?.trim().parse_int()?
  Ok({util: "ls", args: args, status: status, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# The shell owns an inherited regular-file descriptor across the applet exec.
proc invoke_fd(s: uu.Scene, args: List[Str], file: Path) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let captures = test.temp_dir(s.ctx, name: "ls-fd-captures")?
  let out = fp"{captures}/stdout"
  let err = fp"{captures}/stderr"
  let words = uu.argv(s, "ls", [Path(word) for word in args])?
  let argv = [p"/bin/sh", p"-c", Path(r"""exec 3<"$1"; shift; exec "$@"; """), p"ls-fd", file].extend(words)
  let status = process.run(process.command_argv(p"/bin/sh", argv, s.root, stdout: out, stderr: err, timeout: 30s))?.exit_code()?
  Ok({util: "ls", args: args, status: status, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

proc stdout_lacks(r: uu.Ran, needle: Str) [error] -> Result[Unit, Error] {
  let output = r.stdout.utf8()?
  assert !(needle in output), f"unexpected {needle}: {output}"
  Ok()
}

proc stderr_lacks(r: uu.Ran, needle: Str) [error] -> Result[Unit, Error] {
  let output = r.stderr.utf8()?
  assert !(needle in output), f"unexpected {needle}: {output}"
  Ok()
}

# Offset pairs index the full byte stream, including escapes and rendered headers.
proc offset_names(data: Bytes, tag: Str, trim: Bool = false) [error] -> Result[List[Str], Error] {
  let output = data.utf8()?
  let tagged = [line for line in output.lines() if line.starts_with(tag)]
  assert tagged.len() == 1, f"missing or duplicated {tag}: {output}"
  let positions = [word.parse_int()? for word in tagged[0].fields()[1..]]
  assert positions.len() % 2 == 0
  var names: List[Str] = []
  for pair in range(positions.len() / 2) {
    let index = pair * 2
    let name = data[positions[index]..positions[index + 1]].utf8()?
    names += [if trim { name.trim() } else { name }]
  }
  Ok(names)
}

pure reversed_names(names: List[Str]) -> List[Str] {
  [names[names.len() - index - 1] for index in range(names.len())]
}

proc locale_available(s: uu.Scene, name: Str) [fs, process, error] -> Result[Bool, Error] {
  let executable = process.which("locale")
  if let Err(_) = executable { return Ok(false) }
  let captures = test.temp_dir(s.ctx, name: "locale-check")?
  let out = fp"{captures}/stdout"
  let err = fp"{captures}/stderr"
  let status = process.run(process.command_argv(executable?, ["locale", "-a"], s.root, stdout: out, stderr: err, timeout: 2s))?
  if !status.exited_with(0) { return Ok(false) }
  let wanted = name.replace("-", with: "").lower()
  Ok(wanted in [line.replace("-", with: "").lower() for line in out.read_text()?.lines()])
}

# origin: uutils test_ls::test_ls_deref_command_line_dir
test test_uu_ls_ls_deref_command_line_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "some_dir")?
  uu.touch(s, "some_dir/nested_file")?
  uu.symlink(s, uu.at(s, "some_dir").display(), "sym_dir")?
  {
  let r = invoke(s, ["sym_dir"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "nested_file")
  
  }
  {
  let r = invoke(s, ["-l", "sym_dir"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "sym_dir ->")
  
  }
  {
  let r = invoke(s, ["--dereference-command-line-symlink-to-dir", "sym_dir"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "nested_file")
  
  }
  {
  let r = invoke(s, ["-l", "--dereference-command-line-symlink-to-dir", "sym_dir"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "nested_file")
  
  }
  {
  let r = invoke(s, ["--dereference-command-line", "sym_dir"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "nested_file")
  
  }
  {
  let r = invoke(s, ["-l", "--dereference-command-line", "sym_dir"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "nested_file")
  
  }
  {
  let r = invoke(s, ["-lH", "sym_dir"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "nested_file")
  
  }
  {
  let r = invoke(s, ["-l", "--dereference-command-line"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "sym_dir ->")
  
  }
  {
  let r = invoke(s, ["-lH"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "sym_dir ->")
  
  }
  {
  let r = invoke(s, ["-l", "--dereference-command-line-symlink-to-dir"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "sym_dir ->")
  
  }
  {
  let r = invoke(s, ["-l", "--directory", "sym_dir"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "sym_dir ->")
  
  }
  {
  let r = invoke(s, ["-l", "--directory", "--dereference-command-line-symlink-to-dir", "sym_dir"])?
  uu.succeeds(r)
  assert !r.stdout.utf8()?.ends_with("sym_dir")
  
  }
  {
  let r = invoke(s, ["-l", "--directory", "sym_dir"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "sym_dir ->")
  
  }
  {
  let r = invoke(s, ["-l", "--directory", "--dereference-command-line-symlink-to-dir", "sym_dir"])?
  uu.succeeds(r)
  assert !r.stdout.utf8()?.ends_with("sym_dir")
  
  }
}

# origin: uutils test_ls::test_ls_dereference_looped_symlinks_recursive
test test_uu_ls_ls_dereference_looped_symlinks_recursive { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "loop")?
  uu.symlink(s, "../loop", "loop/sub")?
  let r = invoke(s, ["-RL", "loop"])?
  uu.fails_with_code(r, 2)
  uu.stderr_contains(r, "not listing already-listed directory")
}

# origin: uutils test_ls::test_ls_devices
test test_uu_ls_ls_devices { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "some-dir1")?
  {
  let r = invoke_pipe(s, ["-al", "/dev/null"])?
  uu.succeeds(r)
  assert rx"[^ ] 1, 3 [^ ]".matches(r.stdout.utf8()?)
  
  }
  {
  let r = invoke_pipe(s, ["-alL", "/dev/null", "/dev/stdout"])?
  uu.succeeds(r)
  let lines = r.stdout.utf8()?.lines()
  assert lines[0].ends_with("/dev/null")
  assert lines[1].ends_with("/dev/stdout")
  assert lines[0].byte_len() - 9 == lines[1].byte_len() - 11
  
  }
}

# origin: uutils test_ls::test_ls_directory
test test_uu_ls_ls_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "some_dir")?
  uu.touch(s, "some_dir/nested_file")?
  uu.symlink(s, uu.at(s, "some_dir").display(), "sym_dir")?
  {
  let r = invoke(s, ["some_dir"])?
  uu.succeeds(r)
  uu.stdout_is(r, "nested_file\n")
  
  }
  {
  let r = invoke(s, ["--directory", "some_dir"])?
  uu.succeeds(r)
  uu.stdout_is(r, "some_dir\n")
  
  }
  {
  let r = invoke(s, ["sym_dir"])?
  uu.succeeds(r)
  uu.stdout_is(r, "nested_file\n")
  
  }
}

# origin: uutils test_ls::test_ls_directory_dangling_symlink_uses_ln_when_or_blank
test test_uu_ls_ls_directory_dangling_symlink_uses_ln_when_or_blank { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.symlink(s, uu.at(s, "nowhere").display(), "dir/entry")?
  let r = invoke(s, ["--color=always", "dir"], vars: {LS_COLORS: "ln=1;36:or=:"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "\x1b[0m\x1b[1;36mentry\x1b[0m")
}

# origin: uutils test_ls::test_ls_dired_and_zero_are_incompatible
test test_uu_ls_ls_dired_and_zero_are_incompatible { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  {
  let r = invoke(s, ["--dired", "-l", "--zero"])?
  uu.fails_with_code(r, 2)
  uu.stderr_contains(r, "--dired and --zero are incompatible")
  
  }
  {
  let r = invoke(s, ["--dired", "-C", "--zero", "a"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\0")
  
  }
}

# origin: uutils test_ls::test_ls_dired_complex
test test_uu_ls_ls_dired_complex { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.mkdir(s, "d/d")?
  uu.touch(s, "d/a1")?
  uu.touch(s, "d/a22")?
  uu.touch(s, "d/a333")?
  uu.touch(s, "d/a4444")?
  let r = invoke(s, ["--dired", "-l", "d"])?
  uu.succeeds(r)
  if uu.size(s, "d/d")? == 4096 { uu.stdout_contains(r, "  total 4") }
  assert offset_names(r.stdout, "//DIRED//", trim: true)? == ["a1", "a22", "a333", "a4444", "d"]
}

# origin: uutils test_ls::test_ls_dired_format_precedence_is_positional
test test_uu_ls_ls_dired_format_precedence_is_positional { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  for format in ["-C", "--format=single-column"] {
    let r = invoke(s, ["--dired", format, "a"])?
    uu.succeeds(r)
    uu.stdout_only(r, "a\n")
  }
  for args in [["-C", "--dired", "a"], ["--dired", "-1", "a"], ["--dired", "-C", "-g", "a"]] {
    let r = invoke(s, args)?
    uu.succeeds(r)
    assert offset_names(r.stdout, "//DIRED//")? == ["a"]
  }
}

# origin: uutils test_ls::test_ls_dired_hyperlink
test test_uu_ls_ls_dired_hyperlink { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/a")?
  {
  let r = invoke(s, ["--dired", "--hyperlink", "-R"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "file://")
  uu.stdout_contains(r, "-rw")
  stdout_lacks(r, "//DIRED//")?
  
  }
  {
  let r = invoke(s, ["--hyperlink", "--dired", "-R"])?
  uu.succeeds(r)
  stdout_lacks(r, "file://")?
  uu.stdout_contains(r, "//DIRED//")
  
  }
}

# origin: uutils test_ls::test_ls_dired_implies_long
test test_uu_ls_ls_dired_implies_long { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = invoke(s, ["--dired"])?
  uu.succeeds(r)
  stdout_lacks(r, "//DIRED//")?
  uu.stdout_contains(r, "  total 0")
  uu.stdout_contains(r, "//DIRED-OPTIONS// --quoting-style")
  
  }
}

# origin: uutils test_ls::test_ls_dired_leading_info_offsets
test test_uu_ls_ls_dired_leading_info_offsets { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "bay")?
  uu.touch(s, "bay/quill")?
  uu.touch(s, "bay/parchment")?
  for leading in [["-i"], ["-s"], ["-i", "-s"]] {
    let args = ["--dired", "-l", "--quoting-style=literal"].extend(leading).extend(["bay"])
    let r = invoke(s, args)?
    uu.succeeds(r)
    assert offset_names(r.stdout, "//DIRED//")? == ["parchment", "quill"]
  }
}

# origin: uutils test_ls::test_ls_dired_lookalike_operand
test test_uu_ls_ls_dired_lookalike_operand { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "-D")?
  {
  let r = invoke(s, ["--zero", "--", "-D"])?
  uu.succeeds(r)
  uu.stdout_only(r, "-D\0")
  
  }
}

# origin: uutils test_ls::test_ls_dired_name_boundaries
test test_uu_ls_ls_dired_name_boundaries { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.mkdir(s, "d/sub")?
  uu.touch(s, "d/target")?
  uu.touch(s, "d/with space")?
  uu.symlink(s, "target", "d/link")?
  for case in [
    {args: ["--quoting-style=literal", "--color=always"], names: ["link", "sub", "target", "with space"]},
    {args: ["--quoting-style=literal", "-F", "--color=never"], names: ["link", "sub", "target", "with space"]},
    {args: ["--quoting-style=literal", "-F", "--color=always"], names: ["link", "sub", "target", "with space"]},
    {args: ["--quoting-style=shell-escape"], names: ["link", "sub", "target", "'with space'"]},
  ] {
    let r = invoke(s, ["--dired", "-l"].extend(case.args).extend(["d"]))?
    uu.succeeds(r)
    assert offset_names(r.stdout, "//DIRED//")? == case.names
  }
}

# origin: uutils test_ls::test_ls_dired_normal_style_offsets
test test_uu_ls_ls_dired_normal_style_offsets { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.mkdir(s, "d/box")?
  uu.touch(s, "d/note")?
  let r = invoke(s, ["--dired", "-l", "--quoting-style=literal", "--color=always", "d"], vars: {LS_COLORS: "no=35:di=36"})?
  uu.succeeds(r)
  assert r.stdout.utf8()?.starts_with("  total 0\n")
  uu.stdout_contains(r, "//DIRED// 55 58 104 108\n")
  # GNU's offsets index the uncolored listing even when normal style emits SGR prefixes.
  let uncolored = bytes.from_text(rx"\x1b\[[0-9;]*m".replace(r.stdout.utf8()?, with: ""))
  assert offset_names(uncolored, "//DIRED//")? == ["box", "note"]
}

# origin: uutils test_ls::test_ls_dired_offsets_follow_quoted_dir_headers
test test_uu_ls_ls_dired_offsets_follow_quoted_dir_headers { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a b")?
  uu.mkdir(s, "it's")?
  uu.touch(s, "a b/x")?
  uu.touch(s, "it's/y")?
  let r = invoke(s, ["--dired", "-R", "--quoting-style=shell-escape", "a b", "it's"])?
  uu.succeeds(r)
  assert offset_names(r.stdout, "//DIRED//")? == ["x", "y"]
  assert offset_names(r.stdout, "//SUBDIRED//")? == ["'a b'", "\"it's\""]
}

# origin: uutils test_ls::test_ls_dired_order_format
test test_uu_ls_ls_dired_order_format { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/a")?
  {
  let r = invoke(s, ["--dired", "--format=vertical", "-R"])?
  uu.succeeds(r)
  stdout_lacks(r, "//DIRED//")?
  
  }
  {
  let r = invoke(s, ["--format=vertical", "--dired", "-R"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "//DIRED//")
  
  }
}

# origin: uutils test_ls::test_ls_dired_outputs_parent_offset
test test_uu_ls_ls_dired_outputs_parent_offset { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.mkdir(s, "dir/a")?
  {
  let r = invoke(s, ["--dired", "dir", "-R"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "//DIRED//")
  
  }
}

# origin: uutils test_ls::test_ls_dired_outputs_same_date_time_format
test test_uu_ls_ls_dired_outputs_same_date_time_format { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.mkdir(s, "dir/a")?
  let long = invoke(s, ["-l", "dir"])?
  uu.succeeds(long)
  let listing_line = long.stdout.utf8()?.split("\n")[1]
  let r = invoke(s, ["--dired", "dir", "-R"])?
  uu.succeeds(r)
  uu.stdout_contains(r, listing_line)
}

# origin: uutils test_ls::test_ls_dired_position_vs_hyperlink
test test_uu_ls_ls_dired_position_vs_hyperlink { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  {
  let r = invoke(s, ["-C", "--dired", "--hyperlink=never", "a"])?
  uu.succeeds(r)
  assert offset_names(r.stdout, "//DIRED//")? == ["a"]
  
  }
  {
  let r = invoke(s, ["-C", "--dired", "--hyperlink", "a"])?
  uu.succeeds(r)
  assert rx"^-([r-][w-][xt-]){3}".matches(r.stdout.utf8()?)
  uu.stdout_contains(r, "file://")
  stdout_lacks(r, "//DIRED//")?
  
  }
}

# origin: uutils test_ls::test_ls_dired_quoting_style_name
test test_uu_ls_ls_dired_quoting_style_name { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  for style in ["literal", "shell", "shell-always", "shell-escape", "shell-escape-always", "c", "escape", "locale", "clocale"] {
    let r = invoke(s, ["-l", "--dired", f"--quoting-style={style}", "dir"], vars: {LC_ALL: "C"})?
    uu.succeeds(r)
    uu.stdout_contains(r, f"//DIRED-OPTIONS// --quoting-style={style}")
  }
  for case in [{option: "-N", style: "literal"}, {option: "-Q", style: "c"}, {option: "-b", style: "escape"}] {
    let r = invoke(s, ["-l", "--dired", case.option, "dir"], vars: {LC_ALL: "C"})?
    uu.succeeds(r)
    uu.stdout_contains(r, f"//DIRED-OPTIONS// --quoting-style={case.style}")
  }
}

# origin: uutils test_ls::test_ls_dired_recursive
test test_uu_ls_ls_dired_recursive { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = invoke(s, ["--dired", "-l", "-R"])?
  uu.succeeds(r)
  stdout_lacks(r, "//DIRED//")?
  uu.stdout_contains(r, "  total 0")
  uu.stdout_contains(r, "//SUBDIRED// 2 3")
  uu.stdout_contains(r, "//DIRED-OPTIONS// --quoting-style")
  
  }
}

# origin: uutils test_ls::test_ls_dired_recursive_multiple
test test_uu_ls_ls_dired_recursive_multiple { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.mkdir(s, "d/d1")?
  uu.mkdir(s, "d/d2")?
  uu.touch(s, "d/d2/a")?
  uu.touch(s, "d/d2/c2")?
  uu.touch(s, "d/d1/f1")?
  uu.touch(s, "d/d1/file-long")?
  let r = invoke(s, ["--dired", "-l", "-R", "d"])?
  uu.succeeds(r)
  assert offset_names(r.stdout, "//DIRED//", trim: true)? == ["d1", "d2", "f1", "file-long", "a", "c2"]
}

# origin: uutils test_ls::test_ls_dired_simple
test test_uu_ls_ls_dired_simple { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = invoke(s, ["--dired", "-l"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "  total 0")
  
  }
  uu.mkdir(s, "d")?
  uu.touch(s, "d/a1")?
  let r = invoke(s, ["--dired", "-l", "d"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "  total 0")
  assert offset_names(r.stdout, "//DIRED//")? == ["a1"]
}

# origin: uutils test_ls::test_ls_dired_subdired_multiple_dirs_non_recursive
test test_uu_ls_ls_dired_subdired_multiple_dirs_non_recursive { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.mkdir(s, "dir2")?
  uu.touch(s, "dir1/a")?
  uu.touch(s, "dir2/b")?
  let r = invoke(s, ["--dired", "dir1", "dir2"])?
  uu.succeeds(r)
  assert offset_names(r.stdout, "//SUBDIRED//", trim: true)? == ["dir1", "dir2"]
}

# origin: uutils test_ls::test_ls_dired_symlink_name_only
test test_uu_ls_ls_dired_symlink_name_only { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.touch(s, "d/target")?
  uu.symlink(s, "target", "d/link")?
  let r = invoke(s, ["--dired", "-l", "--color=never", "d"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "link -> target")
  assert offset_names(r.stdout, "//DIRED//", trim: true)? == ["link", "target"]
}

# origin: uutils test_ls::test_ls_dired_terminal_keeps_default_quoting
test test_uu_ls_ls_dired_terminal_keeps_default_quoting { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a b")?
  uu.touch(s, "a b/x")?
  let r = invoke(s, ["--dired", "-R", "a b"], terminal: true)?
  uu.succeeds(r)
  let normalized = bytes.from_text(r.stdout.utf8()?.replace("\r\n", with: "\n"))
  assert offset_names(normalized, "//DIRED//")? == ["x"]
  assert offset_names(normalized, "//SUBDIRED//")? == ["'a b'"]
  uu.stdout_contains(r, "//DIRED-OPTIONS// --quoting-style=shell-escape")
}

# origin: uutils test_ls::test_ls_files_dirs
test test_uu_ls_ls_files_dirs { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "a/b")?
  uu.mkdir(s, "a/b/c")?
  uu.mkdir(s, "z")?
  uu.touch(s, "a/a")?
  uu.touch(s, "a/b/b")?
  {
  let r = invoke(s, ["a"])?
  uu.succeeds(r)
  
  }
  {
  let r = invoke(s, ["a/a"])?
  uu.succeeds(r)
  
  }
  {
  let r = invoke(s, ["a", "z"])?
  uu.succeeds(r)
  
  }
  {
  let r = invoke(s, ["doesntexist"])?
  uu.fails(r)
  uu.stderr_contains(r, "'doesntexist': No such file or directory")
  
  }
  {
  let r = invoke(s, ["a", "doesntexist"])?
  uu.fails(r)
  uu.stderr_contains(r, "'doesntexist': No such file or directory")
  uu.stdout_contains(r, "a:")
  
  }
}

# origin: uutils test_ls::test_ls_group_directories_first
test test_uu_ls_ls_group_directories_first { |ctx|
  let s = uu.scene(ctx)?
  let files = ["abc", "anotherFile", "file1", "file2", "xxx", "zzz"]
  for index in range(files.len()) { uu.write(s, files[index], ["a" for _ in range(index)].join(""))? }
  let dirs = ["aaa", "bbb", "ccc", "yyy"]
  for directory in dirs { uu.mkdir(s, directory)? }
  let dots = [".", ".."]
  {
    let r = invoke(s, ["-1a", "--group-directories-first"])?
    uu.succeeds(r)
    assert r.stdout.utf8()?.lines() == dots.extend(dirs).extend(files)
  }
  {
    let r = invoke(s, ["-1", "--group-directories-first", "--sort=size"])?
    uu.succeeds(r)
    assert r.stdout.utf8()?.lines() == dirs.extend(reversed_names(files))
  }
  {
    let r = invoke(s, ["-1ar", "--group-directories-first"])?
    uu.succeeds(r)
    assert r.stdout.utf8()?.lines() == reversed_names(dirs).extend(reversed_names(dots)).extend(reversed_names(files))
  }
  let grouped = invoke(s, ["-1aU", "--group-directories-first"])?
  uu.succeeds(grouped)
  let unsorted = invoke(s, ["-1aU"])?
  uu.succeeds(unsorted)
  assert grouped.stdout == unsorted.stdout
}

# origin: uutils test_ls::test_ls_help
test test_uu_ls_ls_help { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = invoke(s, ["--help"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "--version")
  uu.no_stderr(r)
  
  }
}

# origin: uutils test_ls::test_ls_human_si
test test_uu_ls_ls_human_si { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_human-1")?
  uu.truncate(s, "test_human-1", 1000)?
  {
  let r = invoke(s, ["-hl", "test_human-1"])?
  uu.succeeds(r)
  uu.stdout_contains(r, " 1000 ")
  
  }
  {
  let r = invoke(s, ["-l", "--si", "test_human-1"])?
  uu.succeeds(r)
  uu.stdout_contains(r, " 1.0k ")
  
  }
  uu.truncate(s, "test_human-1", 1000 + 1000 * 1024)?
  {
  let r = invoke(s, ["-hl", "test_human-1"])?
  uu.succeeds(r)
  uu.stdout_contains(r, " 1001K ")
  
  }
  {
  let r = invoke(s, ["-l", "--si", "test_human-1"])?
  uu.succeeds(r)
  uu.stdout_contains(r, " 1.1M ")
  
  }
  uu.touch(s, "test-human-2")?
  uu.truncate(s, "test-human-2", 12300 * 1024)?
  {
  let r = invoke(s, ["-hl", "test-human-2"])?
  uu.succeeds(r)
  uu.stdout_contains(r, " 13M ")
  
  }
  {
  let r = invoke(s, ["-l", "--si", "test-human-2"])?
  uu.succeeds(r)
  uu.stdout_contains(r, " 13M ")
  
  }
  uu.touch(s, "test-human-3")?
  uu.truncate(s, "test-human-3", 9999)?
  {
  let r = invoke(s, ["-hl", "test-human-3"])?
  uu.succeeds(r)
  uu.stdout_contains(r, " 9.8K ")
  
  }
  {
  let r = invoke(s, ["-l", "--si", "test-human-3"])?
  uu.succeeds(r)
  uu.stdout_contains(r, " 10k ")
  
  }
}

# origin: uutils test_ls::test_ls_hyperlink
test test_uu_ls_ls_hyperlink { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a.txt")?
  for option in ["--hyperlink", "--hyperlink=always"] {
    let r = invoke(s, [option])?
    uu.succeeds(r)
    uu.stdout_contains(r, "\x1b]8;;file://")
    uu.stdout_contains(r, f"{s.root}/a.txt\x1b\\a.txt\x1b]8;;\x1b\\")
  }
  for option in ["--hyperlink=never", "--hyperlink=neve", "--hyperlink=ne", "--hyperlink=n"] {
    let r = invoke(s, [option])?
    uu.succeeds(r)
    uu.stdout_is(r, "a.txt\n")
  }
}

# origin: uutils test_ls::test_ls_hyperlink_dirs
test test_uu_ls_ls_hyperlink_dirs { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "b")?
  let r = invoke(s, ["--hyperlink", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "\x1b]8;;file://")
  let lines = r.stdout.utf8()?.lines()
  assert f"{s.root}/a\x1b\\a\x1b]8;;\x1b\\:" in lines[0]
  assert lines[1] == ""
  assert f"{s.root}/b\x1b\\b\x1b]8;;\x1b\\:" in lines[2]
}

# origin: uutils test_ls::test_ls_hyperlink_encode_link
test test_uu_ls_ls_hyperlink_encode_link { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "back\\slash")?
  uu.touch(s, "ques?tion")?
  uu.touch(s, "encoded%3Fquestion")?
  uu.touch(s, "sp ace")?
  let r = invoke(s, ["--hyperlink"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "back%5cslash\x1b\\back\\slash\x1b]8;;\x1b\\")
  uu.stdout_contains(r, "ques%3ftion\x1b\\ques?tion\x1b]8;;\x1b\\")
  uu.stdout_contains(r, "encoded%253Fquestion\x1b\\encoded%3Fquestion\x1b]8;;\x1b\\")
  uu.stdout_contains(r, "sp%20ace\x1b\\sp ace\x1b]8;;\x1b\\")
}

# origin: uutils test_ls::test_ls_hyperlink_recursive_dirs
test test_uu_ls_ls_hyperlink_recursive_dirs { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "a/b")?
  let r = invoke(s, ["--hyperlink", "--recursive", "a"])?
  uu.succeeds(r)
  let lines = r.stdout.utf8()?.lines()
  assert lines[0].starts_with("\x1b]8;;file://")
  assert lines[0].ends_with(f"{s.root}/a\x1b\\a\x1b]8;;\x1b\\:")
  assert lines[1].starts_with("\x1b]8;;file://")
  assert lines[1].ends_with(f"{s.root}/a/b\x1b\\b\x1b]8;;\x1b\\")
  assert lines[2] == ""
  assert lines[3].starts_with("\x1b]8;;file://")
  assert lines[3].ends_with(f"{s.root}/a/b\x1b\\a/b\x1b]8;;\x1b\\:")
}

# origin: uutils test_ls::test_ls_hyperlink_utf8_encoding
test test_uu_ls_ls_hyperlink_utf8_encoding { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "café.txt")?
  uu.touch(s, "file:with:colons.txt")?
  uu.touch(s, "file with spaces.txt")?
  let r = invoke(s, ["--hyperlink"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "caf%c3%a9.txt")
  uu.stdout_contains(r, "file%3awith%3acolons.txt")
  uu.stdout_contains(r, "file%20with%20spaces.txt")
  let output = r.stdout.utf8()?
  assert output.split("\x1b\\").len() - 1 == (output.split("\x1b]8;;file://").len() - 1) * 2
}

# origin: uutils test_ls::test_ls_i
test test_uu_ls_ls_i { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = invoke(s, ["-i"])?
  uu.succeeds(r)
  
  }
  {
  let r = invoke(s, ["-il"])?
  uu.succeeds(r)
  
  }
}

# origin: uutils test_ls::test_ls_ignore_backups
test test_uu_ls_ls_ignore_backups { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "somefile")?
  uu.touch(s, "somebackup~")?
  uu.touch(s, ".somehiddenfile")?
  uu.touch(s, ".somehiddenbackup~")?
  {
  let r = invoke(s, ["-B"])?
  uu.succeeds(r)
  uu.stdout_is(r, "somefile\n")
  
  }
  {
  let r = invoke(s, ["--ignore-backups"])?
  uu.succeeds(r)
  uu.stdout_is(r, "somefile\n")
  
  }
  {
  let r = invoke(s, ["-aB"])?
  uu.succeeds(r)
  uu.stdout_contains(r, ".somehiddenfile")
  uu.stdout_contains(r, "somefile")
  stdout_lacks(r, "somebackup")?
  stdout_lacks(r, ".somehiddenbackup~")?
  
  }
  {
  let r = invoke(s, ["-a", "--ignore-backups"])?
  uu.succeeds(r)
  uu.stdout_contains(r, ".somehiddenfile")
  uu.stdout_contains(r, "somefile")
  stdout_lacks(r, "somebackup")?
  stdout_lacks(r, ".somehiddenbackup~")?
  
  }
}

# origin: uutils test_ls::test_ls_ignore_explicit_period
test test_uu_ls_ls_ignore_explicit_period { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, ".hidden.yml")?
  uu.touch(s, "regular.yml")?
  {
  let r = invoke(s, ["-a", "--ignore", "?hidden.yml"])?
  uu.succeeds(r)
  uu.stdout_contains(r, ".hidden.yml")
  uu.stdout_contains(r, "regular.yml")
  
  }
  {
  let r = invoke(s, ["-a", "--ignore", "*.yml"])?
  uu.succeeds(r)
  uu.stdout_contains(r, ".hidden.yml")
  stdout_lacks(r, "regular.yml")?
  
  }
  {
  let r = invoke(s, ["-a", "--ignore", ".*.yml"])?
  uu.succeeds(r)
  stdout_lacks(r, ".hidden.yml")?
  uu.stdout_contains(r, "regular.yml")
  
  }
}

# origin: uutils test_ls::test_ls_ignore_negation
test test_uu_ls_ls_ignore_negation { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "apple")?
  uu.touch(s, "boy")?
  {
  let r = invoke(s, ["--ignore", "[!a]*"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "apple")
  stdout_lacks(r, "boy")?
  
  }
  {
  let r = invoke(s, ["--ignore", "[^a]*"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "apple")
  stdout_lacks(r, "boy")?
  
  }
}

# origin: uutils test_ls::test_ls_indicator_style
test test_uu_ls_ls_indicator_style { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "directory")?
  uu.touch(s, "link-src")?
  uu.symlink(s, uu.at(s, "link-src").display(), "link-dest.link")?
  assert uu.dir_exists(s, "directory")?
  assert uu.is_symlink(s, "link-dest.link")?
  uu.mkfifo(s, "named-pipe.fifo")?
  assert fs.stat(uu.at(s, "named-pipe.fifo"))?.kind == "fifo"
  for option in ["--indicator-style=classify", "--ind=classify", "--indicator-style=clas", "--indicator-style=c", "--indicator-style=file-type", "--ind=file-type", "--indicator-style=slash", "--ind=slash", "--classify", "--classify=always", "--classify=alway", "--classify=al", "--classify=yes", "--classify=force", "--class", "--file-type", "--file", "-p"] {
    let r = invoke(s, [option])?
    uu.succeeds(r)
    uu.stdout_contains(r, "/")
  }
  for option in ["--indicator-style=none", "--indicator-style=n", "--ind=none", "--classify=none", "--classify=never", "--classify=non", "--classify=no", "--classify=n"] {
    let r = invoke(s, [option])?
    uu.succeeds(r)
    stdout_lacks(r, "/")?
    stdout_lacks(r, "@")?
    stdout_lacks(r, "|")?
  }
  for option in ["classify", "file-type"] {
    let r = invoke(s, [f"--indicator-style={option}"])?
    uu.succeeds(r)
    uu.stdout_contains(r, "@")
    uu.stdout_contains(r, "|")
  }
  let sockets = test.temp_dir(ctx, name: "unix_socket")?
  let constants = linux.net_constants()
  let socket = linux.socket(constants.AF_UNIX, constants.SOCK_STREAM)?
  defer unix.close_fd(socket)
  linux.bind(socket, {family: "unix", address: fp"{sockets}/sock".display()})?
  let r = invoke(s, [sockets.display(), "--indicator-style=classify"])?
  uu.succeeds(r)
  uu.stdout_only(r, "sock=\n")
}

# origin: uutils test_ls::test_ls_indicator_style_filetype_symlink_target_long
test test_uu_ls_ls_indicator_style_filetype_symlink_target_long { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  assert uu.dir_exists(s, "dir")?
  uu.symlink(s, "dir", "dir_link")?
  assert uu.is_symlink(s, "dir_link")?
  let r = invoke(s, ["--file-type", "-l", "dir_link"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "dir_link -> dir/")
  stdout_lacks(r, "dir_link/")?
}

# origin: uutils test_ls::test_ls_indicator_style_filetype_symlink_to_executable_target_long
test test_uu_ls_ls_indicator_style_filetype_symlink_to_executable_target_long { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "exec_target")?
  uu.set_mode(s, "exec_target", 0o755)?
  uu.symlink(s, "exec_target", "link")?
  assert uu.is_symlink(s, "link")?
  let r = invoke(s, ["--file-type", "-l", "link"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "link -> exec_target")
  stdout_lacks(r, "exec_target*")?
}

# origin: uutils test_ls::test_ls_indicator_style_slash_symlink_target_long
test test_uu_ls_ls_indicator_style_slash_symlink_target_long { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  assert uu.dir_exists(s, "dir")?
  uu.symlink(s, "dir", "dir_link")?
  assert uu.is_symlink(s, "dir_link")?
  let r = invoke(s, ["-lp", "dir_link"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "dir_link -> dir\n")
  stdout_lacks(r, "dir_link/")?
  stdout_lacks(r, "-> dir/")?
}

# origin: uutils test_ls::test_ls_indicator_style_symlink_target_long
test test_uu_ls_ls_indicator_style_symlink_target_long { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.symlink(s, uu.at(s, "dir").display(), "dir_link")?
  assert uu.dir_exists(s, "dir")?
  assert uu.is_symlink(s, "dir_link")?
  let r = invoke(s, ["--classify", "-l", "dir_link"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "dir_link -> ")
  stdout_lacks(r, "dir_link@ -> ")?
  uu.stdout_contains(r, "/dir/")
}

# origin: uutils test_ls::test_ls_inode
test test_uu_ls_ls_inode { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_inode")?
  let short_pattern = rx" *(\d+) test_inode"
  let long_pattern = rx" *(\d+) [-bcdlpsDx]([r-][w-][xt-]){3}[.+]? +\d .+ test_inode"
  let short = invoke(s, ["test_inode", "-i"])?
  uu.succeeds(short)
  assert short_pattern.matches(short.stdout.utf8()?)
  let short_inode = short_pattern.captures(short.stdout.utf8()?)[1]
  let plain = invoke(s, ["test_inode"])?
  uu.succeeds(plain)
  assert !short_pattern.matches(plain.stdout.utf8()?)
  stdout_lacks(plain, short_inode)?
  let long = invoke(s, ["-li", "test_inode"])?
  uu.succeeds(long)
  assert long_pattern.matches(long.stdout.utf8()?)
  let long_inode = long_pattern.captures(long.stdout.utf8()?)[1]
  let without = invoke(s, ["-l", "test_inode"])?
  uu.succeeds(without)
  assert !long_pattern.matches(without.stdout.utf8()?)
  stdout_lacks(without, long_inode)?
  assert short_inode == long_inode
}

# origin: uutils test_ls::test_ls_invalid_block_size
test test_uu_ls_ls_invalid_block_size { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = invoke(s, ["--block-size=invalid"])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  uu.stderr_is(r, "ls: invalid --block-size argument 'invalid'\n")
  
  }
  {
  let r = invoke(s, ["--block-size=0"])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  uu.stderr_is(r, "ls: invalid --block-size argument '0'\n")
  
  }
}

# origin: uutils test_ls::test_ls_invalid_block_size_in_env_var
test test_uu_ls_ls_invalid_block_size_in_env_var { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file", ["\0" for _ in range(1024)].join(""))?
  for variable in ["LS_BLOCK_SIZE", "BLOCK_SIZE"] {
    let r = if variable == "LS_BLOCK_SIZE" { invoke(s, ["-og"], vars: {LS_BLOCK_SIZE: "invalid"})? } else { invoke(s, ["-og"], vars: {BLOCK_SIZE: "invalid"})? }
    uu.succeeds(r)
    assert "total 4" in r.stdout.utf8()?.lines()
    uu.stdout_contains(r, " 1 1 ")
  }
  for vars in [{BLOCKSIZE: "invalid"}, {BLOCKSIZE: "0"}] {
    let r = invoke(s, ["-og"], vars: vars)?
    uu.succeeds(r)
    assert "total 4" in r.stdout.utf8()?.lines()
    uu.stdout_contains(r, " 1024 ")
  }
}

# origin: uutils test_ls::test_ls_invalid_quoting_style_env_var_non_utf8
test test_uu_ls_ls_invalid_quoting_style_env_var_non_utf8 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "alpha")?
  let r = invoke(s, ["alpha"], vars: {QUOTING_STYLE: Path.parse_bytes(b"\xff")?})?
  uu.succeeds(r)
  uu.stdout_is(r, "alpha\n")
  uu.stderr_contains(r, "ignoring invalid value of environment variable QUOTING_STYLE")
}

# origin: uutils test_ls::test_ls_invalid_quoting_style_env_var_warns
test test_uu_ls_ls_invalid_quoting_style_env_var_warns { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "alpha")?
  {
  let r = invoke(s, ["alpha"], vars: {QUOTING_STYLE: "not-a-style"})?
  uu.succeeds(r)
  uu.stdout_is(r, "alpha\n")
  uu.stderr_contains(r, "ignoring invalid value of environment variable QUOTING_STYLE")
  
  }
}

# origin: uutils test_ls::test_ls_invalid_quoting_style_env_var_with_unwritable_stderr
test test_uu_ls_ls_invalid_quoting_style_env_var_with_unwritable_stderr { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "zeta")?
  uu.touch(s, "alpha")?
  let r = invoke(s, [], vars: {QUOTING_STYLE: "not-a-style"}, stderr: p"/dev/full")?
  uu.fails_with_code(r, 2)
  uu.stdout_is(r, "alpha\nzeta\n")
}

# origin: uutils test_ls::test_ls_io_errors
test test_uu_ls_ls_io_errors { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "some-dir1")?
  uu.mkdir(s, "some-dir2")?
  uu.mkdir(s, "some-dir3")?
  uu.mkdir(s, "some-dir3/some-dir4")?
  uu.mkdir(s, "some-dir4")?
  uu.symlink(s, uu.at(s, "does_not_exist").display(), "some-dir2/dangle")?
  uu.set_mode(s, "some-dir1", 0o000)?
  defer uu.set_mode(s, "some-dir1", 0o700)
  {
  let r = invoke(s, ["-1", "some-dir1"])?
  uu.fails_with_code(r, 2)
  uu.stderr_contains(r, "cannot open directory")
  uu.stderr_contains(r, "Permission denied")
  
  }
  {
  let r = invoke(s, ["-Li", "some-dir2"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "cannot access")
  uu.stderr_contains(r, "No such file or directory")
  uu.stdout_contains(r, "? dangle")
  
  }
  uu.set_mode(s, "some-dir3/some-dir4", 0o000)?
  defer uu.set_mode(s, "some-dir3/some-dir4", 0o700)
  {
  let r = invoke(s, ["-laR", "some-dir3"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "some-dir4")
  uu.stderr_contains(r, "cannot open directory")
  uu.stderr_contains(r, "Permission denied")
  uu.stdout_contains(r, "some-dir4")
  
  }
  {
  let r = invoke(s, ["-iRL", "some-dir2"])?
  uu.fails(r)
  stderr_lacks(r, "ls: cannot access 'some-dir2/dangle': No such file or directory\nls: cannot access 'some-dir2/dangle': No such file or directory")?
  
  }
  uu.touch(s, "some-dir4/bad-fd.txt")?
  # The inherited descriptor owns a regular file on this Linux test environment.
  for option in ["-alR", "-RiL", "-alL"] {
    let r = invoke_fd(s, [option, "/dev/fd/3"], uu.at(s, "some-dir4/bad-fd.txt"))?
    uu.succeeds(r)
  }
}

# origin: uutils test_ls::test_ls_long
test test_uu_ls_ls_long { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test-long")?
  for option in ["-l", "--long", "--format=long", "--format=lon", "--for=long", "--format=verbose", "--for=verbose"] {
    let r = invoke(s, [option, "test-long"])?
    if option == "--long" {
      uu.fails_with_code(r, 2)
      uu.stderr_only(r, "ls: unrecognized option '--long'\nTry 'ls --help' for more information.\n")
      continue
    }
    uu.succeeds(r)
    assert rx"[-bcCdDlMnpPsStTx?]([r-][w-][xt-]){3}.*".matches(r.stdout.utf8()?)
  }
}

# origin: uutils test_ls::test_ls_long_ctime
test test_uu_ls_ls_long_ctime { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test-long-ctime-1")?
  for option in ["-c", "--time=ctime", "--time=status"] {
    let r = invoke(s, ["-l", option])?
    uu.succeeds(r)
    uu.stdout_contains(r, ":")
  }
}

# origin: uutils test_ls::test_ls_long_dangling_symlink_color
test test_uu_ls_ls_long_dangling_symlink_color { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.symlink(s, uu.at(s, "foo").display(), "dir1/dangling_symlink")?
  let r = invoke(s, ["-l", "--color=always", "dir1/dangling_symlink"], vars: {LS_COLORS: "ln=target:or=40:mi=34"})?
  uu.succeeds(r)
  let name = rx"(?:\x1b\[[0-9;]*m)*\x1b\[([0-9;]*)mdir1/dangling_symlink\x1b\[0m".captures(r.stdout.utf8()?)
  assert !name.is_empty()
  assert name[1] == "40"
  uu.stdout_contains(r, f"\x1b[34m{uu.at(s, "foo").display()}\x1b[0m")
}

# origin: uutils test_ls::test_ls_long_format
test test_uu_ls_ls_long_format { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "test-long-dir")?
  uu.mkdir(s, "test-long-dir/test-long-dir")?
  uu.touch(s, "test-long-dir/test-long-file")?
  for option in ["-l", "--long", "--format=long", "--format=lon", "--for=long", "--format=verbose", "--for=verbose"] {
    let r = invoke(s, [option, "test-long-dir"])?
    if option == "--long" {
      uu.fails_with_code(r, 2)
      uu.stderr_only(r, "ls: unrecognized option '--long'\nTry 'ls --help' for more information.\n")
      continue
    }
    uu.succeeds(r)
    assert rx"\n[-bcCdDlMnpPsStTx?]([r-][w-][xt-]){3}[.+]? +\d+ [^ ]+ +[^ ]+( +[^ ]+)? +\d+ [A-Z][a-z]{2} {0,2}\d{0,2} {0,2}[0-9:]+ ".matches(r.stdout.utf8()?)
  }
  let r = invoke(s, ["-lan", "test-long-dir"])?
  uu.succeeds(r)
  assert rx"\nd([r-][w-][xt-]){3}[.+]? +\d+ \d+ +\d+( +\d+)? +\d+ [A-Z][a-z]{2} {0,2}\d{0,2} {0,2}[0-9:]+ \.\.".matches(r.stdout.utf8()?)
}

# origin: uutils test_ls::test_ls_long_formats
test test_uu_ls_ls_long_formats { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test-long-formats")?
  {
  let r = invoke(s, ["-l", "--author", "test-long-formats"])?
  uu.succeeds(r)
  assert rx"[xrw-]{9}[.+]? \d ([-0-9_a-z.A-Z]+ ){3}0".matches(r.stdout.utf8()?)
  
  }
  {
  let r = invoke(s, ["-l1", "--author", "test-long-formats"])?
  uu.succeeds(r)
  assert rx"[xrw-]{9}[.+]? \d ([-0-9_a-z.A-Z]+ ){3}0".matches(r.stdout.utf8()?)
  
  }
  {
  let r = invoke(s, ["-n", "--author", "test-long-formats"])?
  uu.succeeds(r)
  assert rx"[xrw-]{9}[.+]? \d (\d+ ){3}0".matches(r.stdout.utf8()?)
  
  }
  for option in ["-l", "-g --author", "-o --author", "-lG --author", "-l --no-group --author"] {
    let args = option.split(" ").extend(["test-long-formats"])
    let r = invoke(s, args)?
    uu.succeeds(r)
    assert rx"[xrw-]{9}[.+]? \d ([-0-9_a-z.A-Z]+ ){2}0".matches(r.stdout.utf8()?)
    let numbered = invoke(s, ["-n"].extend(args))?
    uu.succeeds(numbered)
    assert rx"[xrw-]{9}[.+]? \d (\d+ ){2}0".matches(numbered.stdout.utf8()?)
  }
  for option in ["-g", "-gl", "-o", "-ol", "-oG", "-lG", "-l --no-group", "-gG --author"] {
    let args = option.split(" ").extend(["test-long-formats"])
    let r = invoke(s, args)?
    uu.succeeds(r)
    assert rx"[xrw-]{9}[.+]? \d [-0-9_a-z.A-Z]+ 0".matches(r.stdout.utf8()?)
    let numbered = invoke(s, ["-n"].extend(args))?
    uu.succeeds(numbered)
    assert rx"[xrw-]{9}[.+]? \d \d+ 0".matches(numbered.stdout.utf8()?)
  }
  for option in ["-og", "-ogl", "-lgo", "-gG", "-g --no-group", "-og --no-group", "-og --format=long", "-ogCl", "-og --format=vertical -l", "-og1", "-og1l"] {
    let args = option.split(" ").extend(["test-long-formats"])
    let r = invoke(s, args)?
    uu.succeeds(r)
    assert rx"[xrw-]{9}[.+]? \d 0".matches(r.stdout.utf8()?)
    let numbered = invoke(s, ["-n"].extend(args))?
    uu.succeeds(numbered)
    assert rx"[xrw-]{9}[.+]? \d 0".matches(numbered.stdout.utf8()?)
  }
}

# origin: uutils test_ls::test_ls_long_padding_of_size_column_with_multiple_files
test test_uu_ls_ls_long_padding_of_size_column_with_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/a")?
  uu.touch(s, "dir/b")?
  {
  let r = invoke(s, [])?
  
  }
  {
  let r = invoke(s, ["-l", "dir"])?
  uu.succeeds(r)
  uu.stdout_contains(r, " 0 ")
  stdout_lacks(r, "  0 ")?
  
  }
}

# origin: uutils test_ls::test_ls_long_self_referential_dir_lists_contents
test test_uu_ls_ls_long_self_referential_dir_lists_contents { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "real")?
  uu.touch(s, "real/inside")?
  uu.symlink(s, uu.at(s, "real").display(), "link")?
  let cwd = {ctx: ctx, root: uu.at(s, "link")}
  {
  let r = invoke(cwd, ["-l"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "inside")
  stdout_lacks(r, "-> ")?
  
  }
  {
  let r = invoke(s, ["-l", "link"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "link ->")
  
  }
}

# origin: uutils test_ls::test_ls_long_stat_failure_is_reported
test test_uu_ls_ls_long_stat_failure_is_reported { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/file")?
  uu.symlink(s, "/", "dir/link")?
  uu.set_mode(s, "dir", 0o600)?
  defer uu.set_mode(s, "dir", 0o700)
  let r = invoke(s, ["-l", "dir"])?
  uu.fails_with_code(r, 1)
  uu.stdout_contains(r, "? file")
  uu.stdout_contains(r, "? link")
  uu.stderr_contains(r, "cannot access 'dir/file': Permission denied")
  uu.stderr_contains(r, "cannot access 'dir/link': Permission denied")
  uu.set_mode(s, "dir", 0o700)?
}

# origin: uutils test_ls::test_ls_long_total_size
test test_uu_ls_ls_long_total_size { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test-long", "1")?
  uu.write(s, "test-long2", "2")?
  for option in ["-l", "--long", "--format=long", "--format=lon", "--for=long", "--format=verbose", "--for=verbose"] {
    let r = invoke(s, [option])?
    if option == "--long" {
      uu.fails_with_code(r, 2)
      uu.stderr_only(r, "ls: unrecognized option '--long'\nTry 'ls --help' for more information.\n")
      for unit in ["-h", "--human-readable", "--si"] {
        let units = invoke(s, [option, unit])?
        uu.fails_with_code(units, 2)
        uu.stderr_only(units, "ls: unrecognized option '--long'\nTry 'ls --help' for more information.\n")
      }
      continue
    }
    uu.succeeds(r)
    uu.stdout_contains(r, "total 8")
    for unit in ["-h", "--human-readable", "--si"] {
      let units = invoke(s, [option, unit])?
      uu.succeeds(units)
      uu.stdout_contains(units, if unit == "--si" { "total 8.2k" } else { "total 8.0K" })
    }
  }
}

# origin: uutils test_ls::test_ls_ls
test test_uu_ls_ls_ls { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = invoke(s, [])?
  uu.succeeds(r)
  
  }
}

# origin: uutils test_ls::test_ls_multiple_a_A
test test_uu_ls_ls_multiple_a_A { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = invoke(s, ["-a", "-a"])?
  uu.succeeds(r)
  uu.stdout_contains(r, ".")
  uu.stdout_contains(r, "..")
  
  }
  {
  let r = invoke(s, ["-A", "-A"])?
  uu.succeeds(r)
  stdout_lacks(r, ".")?
  stdout_lacks(r, "..")?
  
  }
}

# origin: uutils test_ls::test_ls_non_existing
test test_uu_ls_ls_non_existing { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = invoke(s, ["doesntexist"])?
  uu.fails(r)
  
  }
}

# origin: uutils test_ls::test_ls_non_utf8_hidden
test test_uu_ls_ls_non_utf8_hidden { |ctx|
  let s = uu.scene(ctx)?
  uu.at_bytes(s, b".hidden\x80")?.write("")?
  let r = invoke(s, [])?
  uu.succeeds(r)
  stdout_lacks(r, ".hidden")?
}

# origin: uutils test_ls::test_ls_oneline
test test_uu_ls_ls_oneline { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test-oneline-1")?
  uu.touch(s, "test-oneline-2")?
  for option in ["-1", "--format=single-column"] {
    let r = invoke(s, [option])?
    uu.succeeds(r)
    uu.stdout_only(r, "test-oneline-1\ntest-oneline-2\n")
  }
}

# origin: uutils test_ls::test_ls_only_dirs_formatting
test test_uu_ls_ls_only_dirs_formatting { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "some-dir1")?
  uu.mkdir(s, "some-dir2")?
  uu.mkdir(s, "some-dir3")?
  let r = invoke(s, ["-1", "-R"])?
  uu.succeeds(r)
  uu.stdout_only(r, ".:\nsome-dir1\nsome-dir2\nsome-dir3\n\n./some-dir1:\n\n./some-dir2:\n\n./some-dir3:\n")
}

# origin: uutils test_ls::test_ls_order_mtime
test test_uu_ls_ls_order_mtime { |ctx|
  let s = uu.scene(ctx)?
  # Explicit nanoseconds retain the upstream strict creation-time ordering.
  var stamp = time.now() * 1000000
  for name in ["test-3", "test-4", "test-1", "test-2"] {
    uu.touch(s, name)?
    fs.set_times(uu.at(s, name), mtime_ns: stamp)?
    stamp += 1
  }
  {
  let r = invoke(s, ["-t", "--time=mtime"])?
  uu.succeeds(r)
  uu.stdout_only(r, "test-2\ntest-1\ntest-4\ntest-3\n")
  
  }
  fs.set_times(uu.at(s, "test-3"), mtime_ns: stamp)?
  fs.set_times(uu.at(s, "test-4"), mtime_ns: stamp + 1)?
  let r = invoke(s, ["-t", "--time=mtime"])?
  uu.succeeds(r)
  uu.stdout_only(r, "test-4\ntest-3\ntest-2\ntest-1\n")
}

# origin: uutils test_ls::test_ls_order_size
test test_uu_ls_ls_order_size { |ctx|
  let s = uu.scene(ctx)?
  for index in range(1, 5) { uu.write(s, f"test-{index}", [f"{index}" for _ in range(index)].join(""))? }
  uu.succeeds(invoke(s, ["-al"])?)
  for args in [["-S"], ["-S", "-r"], ["--sort=size"], ["--sort=siz"], ["--sort=s"], ["--sort=size", "-r"]] {
    let r = invoke(s, args)?
    uu.succeeds(r)
    uu.stdout_only(r, if "-r" in args { "test-1\ntest-2\ntest-3\ntest-4\n" } else { "test-4\ntest-3\ntest-2\ntest-1\n" })
  }
}

# origin: uutils test_ls::test_ls_order_time
test test_uu_ls_ls_order_time { |ctx|
  let s = uu.scene(ctx)?
  for index in range(1, 5) {
    uu.write(s, f"test-{index}", [f"{index}" for _ in range(index)].join(""))?
    time.sleep(100ms)?
  }
  let accessed = uu.read(s, "test-3")?
  let mode = uu.mode(s, "test-2")?
  uu.set_mode(s, "test-2", mode)?
  uu.succeeds(invoke(s, ["-al"])?)
  for args in [["-t"], ["--sort=time"], ["-tr"], ["--sort=time", "-r"]] {
    let r = invoke(s, args)?
    uu.succeeds(r)
    uu.stdout_only(r, if "-r" in args or "-tr" in args { "test-1\ntest-2\ntest-3\ntest-4\n" } else { "test-4\ntest-3\ntest-2\ntest-1\n" })
  }
  for field in ["-u", "-c"] {
    for format in ["-g", "--format=long", "--dired"] {
      let r = invoke(s, [field, format])?
      uu.succeeds(r)
      assert rx"(?s)test-1\n.*test-2\n.*test-3\n.*test-4\n".matches(r.stdout.utf8()?)
    }
  }
  # Access-time updates vary by mount policy; compare the metadata-defined time order.
  let ordered = ["test-1", "test-2", "test-3", "test-4"] |> map { |name| {name: name, stamp: fs.stat(uu.at(s, name))?.atime_ns} } |> sort-by(desc: true) .stamp |> map { |file| f"{file.name}\n" } |> join("")
  for args in [["-t", "-u"], ["-u"], ["-t", "--time=atime"], ["--time=atime"], ["--time=atim"], ["--time=a"], ["-t", "--time=access"], ["--time=access"], ["-t", "--time=use"], ["--time=use"]] {
    let r = invoke(s, args)?
    uu.succeeds(r)
    uu.stdout_only(r, ordered)
    let third = fs.stat(uu.at(s, "test-3"))?.atime_ns
    let fourth = fs.stat(uu.at(s, "test-4"))?.atime_ns
  }
  for option in ["-tc", "-c"] {
    let r = invoke(s, [option])?
    uu.succeeds(r)
    uu.stdout_only(r, "test-2\ntest-4\ntest-3\ntest-1\n")
  }
}

# origin: uutils test_ls::test_ls_order_time_breaks_ties_by_name
test test_uu_ls_ls_order_time_breaks_ties_by_name { |ctx|
  let s = uu.scene(ctx)?
  let names = ["zulu", "alpha", "Mike", "bravo"]
  for name in names { uu.write(s, name, "x")? }
  for name in names { fs.set_times(uu.at(s, name), atime_ns: 1700000000000000000, mtime_ns: 1700000000000000000)? }
  {
  let r = invoke(s, ["-t"], vars: {LC_ALL: "C"})?
  uu.succeeds(r)
  uu.stdout_only(r, "Mike\nalpha\nbravo\nzulu\n")
  }
  {
  let r = invoke(s, ["-tr"], vars: {LC_ALL: "C"})?
  uu.succeeds(r)
  uu.stdout_only(r, "zulu\nbravo\nalpha\nMike\n")
  }
  if locale_available(s, "en_US.UTF-8")? {
  let r = invoke(s, ["-t"], vars: {LC_ALL: "en_US.UTF-8"})?
  uu.succeeds(r)
  uu.stdout_only(r, "alpha\nbravo\nMike\nzulu\n")
  }
}

# origin: uutils test_ls::test_ls_ordering
test test_uu_ls_ls_ordering { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "some-dir1")?
  uu.mkdir(s, "some-dir2")?
  uu.mkdir(s, "some-dir3")?
  uu.mkdir(s, "some-dir4")?
  uu.mkdir(s, "some-dir5")?
  uu.mkdir(s, "some-dir6")?
  {
  let r = invoke(s, ["-Rl"])?
  uu.succeeds(r)
  assert rx"some-dir1:\ntotal 0".matches(r.stdout.utf8()?)
  
  }
}

# origin: uutils test_ls::test_ls_path
test test_uu_ls_ls_path { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "file1")?
  uu.touch(s, "dir/file2")?
  {
  let r = invoke(s, ["dir/file2"])?
  uu.succeeds(r)
  uu.stdout_is(r, "dir/file2\n")
  
  }
  {
  let r = invoke(s, ["./dir/file2"])?
  uu.succeeds(r)
  uu.stdout_is(r, "./dir/file2\n")
  
  }
  let absolute = uu.at(s, "dir/file2").display()
  {
  let r = invoke(s, [absolute])?
  uu.succeeds(r)
  uu.stdout_is(r, f"{absolute}\n")
  
  }
  {
  let r = invoke(s, ["file1", "dir/file2"])?
  uu.succeeds(r)
  uu.stdout_is(r, "dir/file2\nfile1\n")
  
  }
}

# origin: uutils test_ls::test_ls_perm_io_errors
test test_uu_ls_ls_perm_io_errors { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.touch(s, "d/f")?
  uu.symlink(s, "/", "d/s")?
  uu.set_mode(s, "d", 0o600)?
  defer uu.set_mode(s, "d", 0o700)
  let r = invoke(s, ["-l", "d"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "Permission denied")
  uu.stdout_contains(r, "total 0")
  uu.stdout_contains(r, "l????????? ? ? ? ?            ? s")
  uu.stdout_contains(r, "-????????? ? ? ? ?            ? f")
}

# origin: uutils test_ls::test_ls_proc_self_fd_no_errors
test test_uu_ls_ls_proc_self_fd_no_errors { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = invoke(s, ["-l", "/proc/self/fd"])?
  uu.succeeds(r)
  stderr_lacks(r, "cannot access")?
  
  }
}

# origin: uutils test_ls::test_ls_quoting
test test_uu_ls_ls_quoting { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "'need quoting'", "symlink")?
  {
  let r = invoke(s, ["-l", "--quoting-style=shell-escape", "symlink"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "'need quoting'")
  
  }
}

# origin: uutils test_ls::test_ls_quoting_and_color
test test_uu_ls_ls_quoting_and_color { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "one two")?
  {
  let r = invoke(s, ["--color", "one two"])?
  uu.succeeds(r)
  uu.stdout_only(r, "one two\n")
  
  }
  {
  let r = invoke(s, ["--color", "one two"], terminal: true)?
  uu.succeeds(r)
  uu.stdout_only(r, "'one two'\r\n")
  
  }
}

# origin: uutils test_ls::test_ls_quoting_color
test test_uu_ls_ls_quoting_color { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "'need quoting'", "symlink")?
  {
  let r = invoke(s, ["-l", "--quoting-style=shell-escape", "--color=auto", "symlink"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "'need quoting'")
  
  }
}

# origin: uutils test_ls::test_ls_quoting_style
test test_uu_ls_ls_quoting_style { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "one two")?
  uu.touch(s, "one")?
  uu.touch(s, "one\ntwo")?
  uu.touch(s, "one\\two")?
  {
  let r = invoke(s, ["--hide-control-chars", "one\ntwo"])?
  uu.succeeds(r)
  uu.stdout_only(r, "one?two\n")
  
  }
  {
  let r = invoke(s, ["--hide-control-chars", "one\ntwo"], terminal: true)?
  uu.succeeds(r)
  uu.stdout_only(r, "'one'$'\\n''two'\r\n")
  
  }
  for case in [{option: "--quoting-style=literal", output: "one?two"}, {option: "--quoting-style=litera", output: "one?two"}, {option: "--quoting-style=li", output: "one?two"}, {option: "-N", output: "one?two"}, {option: "--literal", output: "one?two"}, {option: "--l", output: "one?two"}, {option: "--quoting-style=c", output: "\"one\\ntwo\""}, {option: "--quoting-style=c-", output: "\"one\\ntwo\""}, {option: "--quoting-style=c-maybe", output: "\"one\\ntwo\""}, {option: "-Q", output: "\"one\\ntwo\""}, {option: "--quote-name", output: "\"one\\ntwo\""}, {option: "--quoting-style=escape", output: "one\\ntwo"}, {option: "--quoting-style=escap", output: "one\\ntwo"}, {option: "-b", output: "one\\ntwo"}, {option: "--escape", output: "one\\ntwo"}, {option: "--quoting-style=shell-escape", output: "'one'$'\\n''two'"}, {option: "--quoting-style=shell-escape-always", output: "'one'$'\\n''two'"}, {option: "--quoting-style=shell-escape-alway", output: "'one'$'\\n''two'"}, {option: "--quoting-style=shell-escape-a", output: "'one'$'\\n''two'"}, {option: "--quoting-style=shell", output: "'one?two'"}, {option: "--quoting-style=shell-always", output: "'one?two'"}, {option: "--quoting-style=shell-a", output: "'one?two'"}] {
    let r = invoke(s, ["--hide-control-chars", case.option, "one\ntwo"])?
    uu.succeeds(r)
    uu.stdout_only(r, f"{case.output}\n")
  }
  
  for case in [{option: "--quoting-style=literal", output: "one\ntwo"}, {option: "-N", output: "one\ntwo"}, {option: "--literal", output: "one\ntwo"}, {option: "--l", output: "one\ntwo"}, {option: "--quoting-style=shell", output: "'one\ntwo'"}, {option: "--quoting-style=shell-always", output: "'one\ntwo'"}] {
    let r = invoke(s, [case.option, "--show-control-chars", "one\ntwo"])?
    uu.succeeds(r)
    uu.stdout_only(r, f"{case.output}\n")
  }
  
  for case in [{option: "--quoting-style=literal", output: "one\\two"}, {option: "-N", output: "one\\two"}, {option: "--quoting-style=c", output: "\"one\\\\two\""}, {option: "-Q", output: "\"one\\\\two\""}, {option: "--quote-name", output: "\"one\\\\two\""}, {option: "--quoting-style=escape", output: "one\\\\two"}, {option: "-b", output: "one\\\\two"}, {option: "--quoting-style=shell-escape", output: "'one\\two'"}, {option: "--quoting-style=shell-escape-always", output: "'one\\two'"}, {option: "--quoting-style=shell", output: "'one\\two'"}, {option: "--quoting-style=shell-always", output: "'one\\two'"}] {
    let r = invoke(s, ["--hide-control-chars", case.option, "one\\two"])?
    uu.succeeds(r)
    uu.stdout_only(r, f"{case.output}\n")
  }
  
  uu.touch(s, "one\n&two")?
  
  for case in [{option: "--quoting-style=shell-escape", output: "'one'$'\\n''&two'"}, {option: "--quoting-style=shell-escape-always", output: "'one'$'\\n''&two'"}] {
    let r = invoke(s, ["--hide-control-chars", case.option, "one\n&two"])?
    uu.succeeds(r)
    uu.stdout_only(r, f"{case.output}\n")
  }
  
  {
  let r = invoke(s, ["one two"])?
  uu.succeeds(r)
  uu.stdout_only(r, "one two\n")
  
  }
  {
  let r = invoke(s, ["one two"], terminal: true)?
  uu.succeeds(r)
  uu.stdout_only(r, "'one two'\r\n")
  
  }
  for case in [{option: "--quoting-style=literal", output: "one two"}, {option: "-N", output: "one two"}, {option: "--literal", output: "one two"}, {option: "--l", output: "one two"}, {option: "--quoting-style=c", output: "\"one two\""}, {option: "-Q", output: "\"one two\""}, {option: "--quote-name", output: "\"one two\""}, {option: "--quoting-style=escape", output: "one\\ two"}, {option: "-b", output: "one\\ two"}, {option: "--escape", output: "one\\ two"}, {option: "--quoting-style=shell-escape", output: "'one two'"}, {option: "--quoting-style=shell-escape-always", output: "'one two'"}, {option: "--quoting-style=shell", output: "'one two'"}, {option: "--quoting-style=shell-always", output: "'one two'"}] {
    let r = invoke(s, ["--hide-control-chars", case.option, "one two"])?
    uu.succeeds(r)
    uu.stdout_only(r, f"{case.output}\n")
  }
  
  {
  let r = invoke(s, ["one"])?
  uu.succeeds(r)
  uu.stdout_only(r, "one\n")
  
  }
  for case in [{option: "--quoting-style=literal", output: "one"}, {option: "-N", output: "one"}, {option: "--quoting-style=c", output: "\"one\""}, {option: "-Q", output: "\"one\""}, {option: "--quote-name", output: "\"one\""}, {option: "--quoting-style=escape", output: "one"}, {option: "-b", output: "one"}, {option: "--quoting-style=shell-escape", output: "one"}, {option: "--quoting-style=shell-escape-always", output: "'one'"}, {option: "--quoting-style=shell", output: "one"}, {option: "--quoting-style=shell-always", output: "'one'"}] {
    let r = invoke(s, ["--hide-control-chars", case.option, "one"])?
    uu.succeeds(r)
    uu.stdout_only(r, f"{case.output}\n")
  }
}

# origin: uutils test_ls::test_ls_quoting_style_arg_overrides_env_var
test test_uu_ls_ls_quoting_style_arg_overrides_env_var { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo-1")?
  uu.touch(s, "bar-2")?
  for case in [{option: "--quoting-style=literal", output: "foo-1"}, {option: "-N", output: "foo-1"}, {option: "--quoting-style=escape", output: "foo-1"}, {option: "-b", output: "foo-1"}, {option: "--quoting-style=shell-escape", output: "foo-1"}, {option: "--quoting-style=shell-escape-always", output: "'foo-1'"}, {option: "--quoting-style=shell", output: "foo-1"}, {option: "--quoting-style=shell-always", output: "'foo-1'"}] {
    let r = invoke(s, ["--hide-control-chars", case.option, "foo-1"], vars: {QUOTING_STYLE: "c"})?
    uu.succeeds(r)
    uu.stdout_only(r, f"{case.output}\n")
  }
  for case in [{option: "--quoting-style=c", output: "\"foo-1\""}, {option: "-Q", output: "\"foo-1\""}, {option: "--quote-name", output: "\"foo-1\""}] {
    let r = invoke(s, ["--hide-control-chars", case.option, "foo-1"], vars: {QUOTING_STYLE: "literal"})?
    uu.succeeds(r)
    uu.stdout_only(r, f"{case.output}\n")
  }
}

# origin: uutils test_ls::test_ls_quoting_style_env_var_default
test test_uu_ls_ls_quoting_style_env_var_default { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo-1")?
  uu.touch(s, "bar-2")?
  {
  let r = invoke(s, [], vars: {QUOTING_STYLE: "c"})?
  uu.succeeds(r)
  uu.stdout_only(r, "\"bar-2\"\n\"foo-1\"\n")
  
  }
}

