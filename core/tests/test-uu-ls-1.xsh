##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_ls.rs.

use support.uu as uu

# A bounded reader owns the pipe diagnostic bytes without placing capture paths in the listing.
proc pipe_listing_diagnostic(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let capture = test.temp_dir(s.ctx, name: "diagnostic")?
  defer capture.remove()?
  let pipe = fp"{capture}/pipe"
  fs.mkfifo(pipe, 0o600)?
  let output = fp"{capture}/output"
  let errors = fp"{capture}/reader-error"
  let cat = process.which("cat")?
  let reader = spawn process.command_argv(cat, [cat, pipe], s.root, {}, b"", output, errors, timeout: 5s)?
  defer reader.cancel(kill_after: 100ms)?
  let r = uu.invoke(s, "ls", args, stdout: fp"{capture}/stdout", stderr: pipe, timeout: 5s)?
  assert (wait reader?).exited_with(0)
  assert errors.read_bytes()? == b""
  Ok({...r, stdout: fp"{capture}/stdout".read_bytes()?, stderr: output.read_bytes()?})
}

proc listing_terminal() [process, error] -> Result[UnixPty, Error] {
  let pair = unix.open_pty()?
  unix.set_window_size(30, 80, xpixel: 640, ypixel: 300, fd: pair.replica)?
  Ok(pair)
}

# Keep the replica open until queued terminal bytes have been drained.
proc listing_terminal_bytes(fd: Int) [process, error] -> Result[Bytes, Error] {
  var output = b""
  while "readable" in unix.poll_fd(fd, ["readable"], timeout_ms: 0)? {
    let chunk = unix.read_fd(fd, 8192)?
    break when chunk.is_empty()
    output = bytes.concat([output, chunk])
  }
  Ok(output)
}

proc terminal_listing(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let input = listing_terminal()?
  defer unix.close_fd(input.master)?
  defer unix.close_fd(input.replica)?
  let output = listing_terminal()?
  defer unix.close_fd(output.master)?
  defer unix.close_fd(output.replica)?
  let errors = listing_terminal()?
  defer unix.close_fd(errors.master)?
  defer unix.close_fd(errors.replica)?
  let argv = uu.argv(s, "ls", [Path(word) for word in args])?
  let plan = process.command_argv(s.ctx.xsh_bin, argv, s.root, {}, Path(input.name), Path(output.name), Path(errors.name), timeout: 5s)
  let status = process.run(plan)?
  Ok({util: "ls", args: args, status: status.exit_code()?, stdout: listing_terminal_bytes(output.master)?, stderr: listing_terminal_bytes(errors.master)?})
}

# Capture files live beside the scene because listing -a must see only its fixtures.
proc ls_run(s: uu.Scene, args: List[Str], vars: Record = {}, stdout: Path? = null) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let capture = test.temp_dir(s.ctx, name: "capture")?
  defer capture.remove()?
  let out = stdout ?? fp"{capture}/stdout"
  let err = fp"{capture}/stderr"
  let r = uu.invoke(s, "ls", args, vars: vars, stdout: out, stderr: err)?
  let output = if stdout == null { out.read_bytes()? } else { b"" }
  Ok({...r, stdout: output, stderr: err.read_bytes()?})
}

type QuotingCheck = {style: Str, regular: Str, directory: Str}

proc quoting_directory(ctx: TestContext, dirname: Str, checks: List[QuotingCheck], extra_args: List[Str]) [fs, process, env, error] -> Result[Unit, Error] {
  for check in checks {
    let s = uu.scene(ctx)?
    uu.mkdir(s, dirname)?
    let here = match check.style {
      "shell-always" | "shell-escape-always" => "'.'",
      "c" => "\".\"",
      _ => ".",
    }
    let expected = f"{here}:\n{check.regular}\n\n{check.directory}:\n"
    let r = ls_run(s, ["-R", f"--quoting-style={check.style}"].extend(extra_args))?
    uu.succeeds(r)
    uu.stdout_is(r, expected)
  }
  Ok()
}

# origin: uutils test_ls::quoting::test_ls_quoting_backslash
test test_uu_ls_quoting_ls_quoting_backslash { |ctx|
  quoting_directory(ctx, "dir\\name", [
    {style: "literal", regular: "dir\\name", directory: "./dir\\name"},
    {style: "shell", regular: "'dir\\name'", directory: "'./dir\\name'"},
    {style: "shell-always", regular: "'dir\\name'", directory: "'./dir\\name'"},
    {style: "shell-escape", regular: "'dir\\name'", directory: "'./dir\\name'"},
    {style: "shell-escape-always", regular: "'dir\\name'", directory: "'./dir\\name'"},
    {style: "c", regular: "\"dir\\\\name\"", directory: "\"./dir\\\\name\""},
    {style: "escape", regular: "dir\\\\name", directory: "./dir\\\\name"},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_backspace
test test_uu_ls_quoting_ls_quoting_backspace { |ctx|
  quoting_directory(ctx, "dir\u{8}name", [
    {style: "shell", regular: "dir?name", directory: "./dir?name"},
    {style: "shell-always", regular: "'dir?name'", directory: "'./dir?name'"},
    {style: "shell-escape", regular: r"""'dir'$'\b''name'""", directory: r"""'./dir'$'\b''name'"""},
    {style: "shell-escape-always", regular: r"""'dir'$'\b''name'""", directory: r"""'./dir'$'\b''name'"""},
    {style: "c", regular: "\"dir\\bname\"", directory: "\"./dir\\bname\""},
    {style: "escape", regular: "dir\\bname", directory: "./dir\\bname"},
  ], ["--hide-control-chars"])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_bell
test test_uu_ls_quoting_ls_quoting_bell { |ctx|
  quoting_directory(ctx, "dir\u{7}name", [
    {style: "shell", regular: "dir?name", directory: "./dir?name"},
    {style: "shell-always", regular: "'dir?name'", directory: "'./dir?name'"},
    {style: "shell-escape", regular: r"""'dir'$'\a''name'""", directory: r"""'./dir'$'\a''name'"""},
    {style: "shell-escape-always", regular: r"""'dir'$'\a''name'""", directory: r"""'./dir'$'\a''name'"""},
    {style: "c", regular: "\"dir\\aname\"", directory: "\"./dir\\aname\""},
    {style: "escape", regular: "dir\\aname", directory: "./dir\\aname"},
  ], ["--hide-control-chars"])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_caret
test test_uu_ls_quoting_ls_quoting_caret { |ctx|
  quoting_directory(ctx, "^-caret", [
    {style: "shell", regular: "'^-caret'", directory: "'./^-caret'"},
    {style: "shell-always", regular: "'^-caret'", directory: "'./^-caret'"},
    {style: "shell-escape", regular: "'^-caret'", directory: "'./^-caret'"},
    {style: "shell-escape-always", regular: "'^-caret'", directory: "'./^-caret'"},
    {style: "c", regular: "\"^-caret\"", directory: "\"./^-caret\""},
    {style: "escape", regular: "^-caret", directory: "./^-caret"},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_carriage_return
test test_uu_ls_quoting_ls_quoting_carriage_return { |ctx|
  quoting_directory(ctx, "dir\rname", [
    {style: "literal", regular: "dir?name", directory: "./dir?name"},
    {style: "shell", regular: "'dir?name'", directory: "'./dir?name'"},
    {style: "shell-always", regular: "'dir?name'", directory: "'./dir?name'"},
    {style: "shell-escape", regular: r"""'dir'$'\r''name'""", directory: r"""'./dir'$'\r''name'"""},
    {style: "shell-escape-always", regular: r"""'dir'$'\r''name'""", directory: r"""'./dir'$'\r''name'"""},
    {style: "c", regular: "\"dir\\rname\"", directory: "\"./dir\\rname\""},
    {style: "escape", regular: "dir\\rname", directory: "./dir\\rname"},
  ], ["--hide-control-chars"])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_close_brace
test test_uu_ls_quoting_ls_quoting_close_brace { |ctx|
  quoting_directory(ctx, "}-close_brace", [
    {style: "shell", regular: "}-close_brace", directory: "./}-close_brace"},
    {style: "shell-always", regular: "'}-close_brace'", directory: "'./}-close_brace'"},
    {style: "shell-escape", regular: "}-close_brace", directory: "./}-close_brace"},
    {style: "shell-escape-always", regular: "'}-close_brace'", directory: "'./}-close_brace'"},
    {style: "c", regular: "\"}-close_brace\"", directory: "\"./}-close_brace\""},
    {style: "escape", regular: "}-close_brace", directory: "./}-close_brace"},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_close_bracket
test test_uu_ls_quoting_ls_quoting_close_bracket { |ctx|
  quoting_directory(ctx, "]-close_bracket", [
    {style: "shell", regular: "]-close_bracket", directory: "./]-close_bracket"},
    {style: "shell-always", regular: "']-close_bracket'", directory: "'./]-close_bracket'"},
    {style: "shell-escape", regular: "]-close_bracket", directory: "./]-close_bracket"},
    {style: "shell-escape-always", regular: "']-close_bracket'", directory: "'./]-close_bracket'"},
    {style: "c", regular: "\"]-close_bracket\"", directory: "\"./]-close_bracket\""},
    {style: "escape", regular: "]-close_bracket", directory: "./]-close_bracket"},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_colon
test test_uu_ls_quoting_ls_quoting_colon { |ctx|
  quoting_directory(ctx, "dir:name", [
    {style: "literal", regular: "dir:name", directory: "./dir:name"},
    {style: "shell", regular: "dir:name", directory: "'./dir:name'"},
    {style: "shell-always", regular: "'dir:name'", directory: "'./dir:name'"},
    {style: "shell-escape", regular: "dir:name", directory: "'./dir:name'"},
    {style: "shell-escape-always", regular: "'dir:name'", directory: "'./dir:name'"},
    {style: "c", regular: "\"dir:name\"", directory: "\"./dir\\:name\""},
    {style: "escape", regular: "dir:name", directory: "./dir\\:name"},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_dollar
test test_uu_ls_quoting_ls_quoting_dollar { |ctx|
  quoting_directory(ctx, r"""dir$name""", [
    {style: "literal", regular: r"""dir$name""", directory: r"""./dir$name"""},
    {style: "shell", regular: r"""'dir$name'""", directory: r"""'./dir$name'"""},
    {style: "shell-always", regular: r"""'dir$name'""", directory: r"""'./dir$name'"""},
    {style: "shell-escape", regular: r"""'dir$name'""", directory: r"""'./dir$name'"""},
    {style: "shell-escape-always", regular: r"""'dir$name'""", directory: r"""'./dir$name'"""},
    {style: "c", regular: r""""dir$name""" + "\"", directory: r""""./dir$name""" + "\""},
    {style: "escape", regular: r"""dir$name""", directory: r"""./dir$name"""},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_double_quote
test test_uu_ls_quoting_ls_quoting_double_quote { |ctx|
  quoting_directory(ctx, "dir\"name", [
    {style: "literal", regular: "dir\"name", directory: "./dir\"name"},
    {style: "shell", regular: "'dir\"name'", directory: "'./dir\"name'"},
    {style: "shell-always", regular: "'dir\"name'", directory: "'./dir\"name'"},
    {style: "shell-escape", regular: "'dir\"name'", directory: "'./dir\"name'"},
    {style: "shell-escape-always", regular: "'dir\"name'", directory: "'./dir\"name'"},
    {style: "c", regular: "\"dir\\\"name\"", directory: "\"./dir\\\"name\""},
    {style: "escape", regular: "dir\"name", directory: "./dir\"name"},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_equal
test test_uu_ls_quoting_ls_quoting_equal { |ctx|
  quoting_directory(ctx, "=-equal", [
    {style: "shell", regular: "'=-equal'", directory: "'./=-equal'"},
    {style: "shell-always", regular: "'=-equal'", directory: "'./=-equal'"},
    {style: "shell-escape", regular: "'=-equal'", directory: "'./=-equal'"},
    {style: "shell-escape-always", regular: "'=-equal'", directory: "'./=-equal'"},
    {style: "c", regular: "\"=-equal\"", directory: "\"./=-equal\""},
    {style: "escape", regular: "=-equal", directory: "./=-equal"},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_formfeed
test test_uu_ls_quoting_ls_quoting_formfeed { |ctx|
  quoting_directory(ctx, "dir\u{c}name", [
    {style: "shell", regular: "dir?name", directory: "./dir?name"},
    {style: "shell-always", regular: "'dir?name'", directory: "'./dir?name'"},
    {style: "shell-escape", regular: r"""'dir'$'\f''name'""", directory: r"""'./dir'$'\f''name'"""},
    {style: "shell-escape-always", regular: r"""'dir'$'\f''name'""", directory: r"""'./dir'$'\f''name'"""},
    {style: "c", regular: "\"dir\\fname\"", directory: "\"./dir\\fname\""},
    {style: "escape", regular: "dir\\fname", directory: "./dir\\fname"},
  ], ["--hide-control-chars"])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_linefeed
test test_uu_ls_quoting_ls_quoting_linefeed { |ctx|
  quoting_directory(ctx, "dir\nname", [
    {style: "literal", regular: "dir?name", directory: "./dir?name"},
    {style: "shell", regular: "'dir?name'", directory: "'./dir?name'"},
    {style: "shell-always", regular: "'dir?name'", directory: "'./dir?name'"},
    {style: "shell-escape", regular: r"""'dir'$'\n''name'""", directory: r"""'./dir'$'\n''name'"""},
    {style: "shell-escape-always", regular: r"""'dir'$'\n''name'""", directory: r"""'./dir'$'\n''name'"""},
    {style: "c", regular: "\"dir\\nname\"", directory: "\"./dir\\nname\""},
    {style: "escape", regular: "dir\\nname", directory: "./dir\\nname"},
  ], [])?
  quoting_directory(ctx, "dir\nname", [
    {style: "literal", regular: "dir?name", directory: "./dir?name"},
    {style: "shell", regular: "'dir?name'", directory: "'./dir?name'"},
    {style: "shell-always", regular: "'dir?name'", directory: "'./dir?name'"},
    {style: "shell-escape", regular: r"""'dir'$'\n''name'""", directory: r"""'./dir'$'\n''name'"""},
    {style: "shell-escape-always", regular: r"""'dir'$'\n''name'""", directory: r"""'./dir'$'\n''name'"""},
    {style: "c", regular: "\"dir\\nname\"", directory: "\"./dir\\nname\""},
    {style: "escape", regular: "dir\\nname", directory: "./dir\\nname"},
  ], ["--hide-control-chars"])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_open_brace
test test_uu_ls_quoting_ls_quoting_open_brace { |ctx|
  quoting_directory(ctx, "{-open_brace", [
    {style: "shell", regular: "{-open_brace", directory: "./{-open_brace"},
    {style: "shell-always", regular: "'{-open_brace'", directory: "'./{-open_brace'"},
    {style: "shell-escape", regular: "{-open_brace", directory: "./{-open_brace"},
    {style: "shell-escape-always", regular: "'{-open_brace'", directory: "'./{-open_brace'"},
    {style: "c", regular: "\"{-open_brace\"", directory: "\"./{-open_brace\""},
    {style: "escape", regular: "{-open_brace", directory: "./{-open_brace"},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_open_bracket
test test_uu_ls_quoting_ls_quoting_open_bracket { |ctx|
  quoting_directory(ctx, "[-open_bracket", [
    {style: "shell", regular: "'[-open_bracket'", directory: "'./[-open_bracket'"},
    {style: "shell-always", regular: "'[-open_bracket'", directory: "'./[-open_bracket'"},
    {style: "shell-escape", regular: "'[-open_bracket'", directory: "'./[-open_bracket'"},
    {style: "shell-escape-always", regular: "'[-open_bracket'", directory: "'./[-open_bracket'"},
    {style: "c", regular: "\"[-open_bracket\"", directory: "\"./[-open_bracket\""},
    {style: "escape", regular: "[-open_bracket", directory: "./[-open_bracket"},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_simple
test test_uu_ls_quoting_ls_quoting_simple { |ctx|
  quoting_directory(ctx, "dirname", [
    {style: "literal", regular: "dirname", directory: "./dirname"},
    {style: "shell", regular: "dirname", directory: "./dirname"},
    {style: "shell-always", regular: "'dirname'", directory: "'./dirname'"},
    {style: "shell-escape", regular: "dirname", directory: "./dirname"},
    {style: "shell-escape-always", regular: "'dirname'", directory: "'./dirname'"},
    {style: "c", regular: "\"dirname\"", directory: "\"./dirname\""},
    {style: "escape", regular: "dirname", directory: "./dirname"},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_single_quote
test test_uu_ls_quoting_ls_quoting_single_quote { |ctx|
  quoting_directory(ctx, "dir'name", [
    {style: "literal", regular: "dir'name", directory: "./dir'name"},
    {style: "shell", regular: "\"dir'name\"", directory: "\"./dir'name\""},
    {style: "shell-always", regular: "\"dir'name\"", directory: "\"./dir'name\""},
    {style: "shell-escape", regular: "\"dir'name\"", directory: "\"./dir'name\""},
    {style: "shell-escape-always", regular: "\"dir'name\"", directory: "\"./dir'name\""},
    {style: "c", regular: "\"dir'name\"", directory: "\"./dir'name\""},
    {style: "escape", regular: "dir'name", directory: "./dir'name"},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_space
test test_uu_ls_quoting_ls_quoting_space { |ctx|
  quoting_directory(ctx, "dir name", [
    {style: "literal", regular: "dir name", directory: "./dir name"},
    {style: "shell", regular: "'dir name'", directory: "'./dir name'"},
    {style: "shell-always", regular: "'dir name'", directory: "'./dir name'"},
    {style: "shell-escape", regular: "'dir name'", directory: "'./dir name'"},
    {style: "shell-escape-always", regular: "'dir name'", directory: "'./dir name'"},
    {style: "c", regular: "\"dir name\"", directory: "\"./dir name\""},
    {style: "escape", regular: "dir\\ name", directory: "./dir name"},
  ], [])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_tabulation
test test_uu_ls_quoting_ls_quoting_tabulation { |ctx|
  quoting_directory(ctx, "dir\tname", [
    {style: "literal", regular: "dir\tname", directory: "./dir\tname"},
    {style: "shell", regular: "'dir\tname'", directory: "'./dir\tname'"},
    {style: "shell-always", regular: "'dir\tname'", directory: "'./dir\tname'"},
    {style: "shell-escape", regular: r"""'dir'$'\t''name'""", directory: r"""'./dir'$'\t''name'"""},
    {style: "shell-escape-always", regular: r"""'dir'$'\t''name'""", directory: r"""'./dir'$'\t''name'"""},
    {style: "c", regular: "\"dir\\tname\"", directory: "\"./dir\\tname\""},
    {style: "escape", regular: "dir\\tname", directory: "./dir\\tname"},
  ], [])?
  quoting_directory(ctx, "dir\tname", [
    {style: "literal", regular: "dir?name", directory: "./dir?name"},
    {style: "shell", regular: "'dir?name'", directory: "'./dir?name'"},
    {style: "shell-always", regular: "'dir?name'", directory: "'./dir?name'"},
    {style: "shell-escape", regular: r"""'dir'$'\t''name'""", directory: r"""'./dir'$'\t''name'"""},
    {style: "shell-escape-always", regular: r"""'dir'$'\t''name'""", directory: r"""'./dir'$'\t''name'"""},
    {style: "c", regular: "\"dir\\tname\"", directory: "\"./dir\\tname\""},
    {style: "escape", regular: "dir\\tname", directory: "./dir\\tname"},
  ], ["--hide-control-chars"])?
}

# origin: uutils test_ls::quoting::test_ls_quoting_vertical_tab
test test_uu_ls_quoting_ls_quoting_vertical_tab { |ctx|
  quoting_directory(ctx, "dir\u{b}name", [
    {style: "shell", regular: "dir?name", directory: "./dir?name"},
    {style: "shell-always", regular: "'dir?name'", directory: "'./dir?name'"},
    {style: "shell-escape", regular: r"""'dir'$'\v''name'""", directory: r"""'./dir'$'\v''name'"""},
    {style: "shell-escape-always", regular: r"""'dir'$'\v''name'""", directory: r"""'./dir'$'\v''name'"""},
    {style: "c", regular: "\"dir\\vname\"", directory: "\"./dir\\vname\""},
    {style: "escape", regular: "dir\\vname", directory: "./dir\\vname"},
  ], ["--hide-control-chars"])?
}

# origin: uutils test_ls::test_directory_in_file
test test_uu_ls_directory_in_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  let r1 = ls_run(s, ["file/missing"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_is(r1, "ls: cannot access 'file/missing': Not a directory\n")
}

# origin: uutils test_ls::test_invalid_flag
test test_uu_ls_invalid_flag { |ctx|
  let s = uu.scene(ctx)?
  let r1 = ls_run(s, ["--invalid-argument"])?
  uu.fails_with_code(r1, 2)
  uu.no_stdout(r1)
}

# origin: uutils test_ls::test_invalid_value_time_style
test test_uu_ls_invalid_value_time_style { |ctx|
  let s = uu.scene(ctx)?
  let r1 = ls_run(s, ["--time-style=definitely_invalid_value"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  let r2 = ls_run(s, ["-g", "--time-style=definitely_invalid_value"])?
  uu.fails_with_code(r2, 2)
  uu.stderr_contains(r2, "invalid argument 'definitely_invalid_value' for 'time style'")
  uu.no_stdout(r2)
  let r3 = ls_run(s, ["-l", "--time-style=definitely_invalid_value", "--format=single-column"])?
  uu.succeeds(r3)
  uu.no_stderr(r3)
}

# origin: uutils test_ls::test_long_options_detached
test test_uu_ls_long_options_detached { |ctx|
  let s = uu.scene(ctx)?
  let r1 = ls_run(s, ["--sort", "name"])?
  uu.succeeds(r1)
  let r2 = ls_run(s, ["--format", "single-column"])?
  uu.succeeds(r2)
  let r3 = ls_run(s, ["--time", "mtime"])?
  uu.succeeds(r3)
  let r4 = ls_run(s, ["--block-size", "512"])?
  uu.succeeds(r4)
}

# origin: uutils test_ls::test_ls_a_A
test test_uu_ls_ls_a_A { |ctx|
  let s = uu.scene(ctx)?
  let r1 = ls_run(s, ["-A", "-a"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, ".")
  uu.stdout_contains(r1, "..")
  let r2 = ls_run(s, ["-a", "-A"])?
  uu.succeeds(r2)
  assert "." not in r2.stdout.utf8()?
  assert ".." not in r2.stdout.utf8()?
}

# origin: uutils test_ls::test_ls_a_dotdot_no_error_on_wasi
test test_uu_ls_ls_a_dotdot_no_error_on_wasi { |ctx|
  let s = uu.scene(ctx)?
  let r1 = ls_run(s, ["-a", "-1"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "..")
  uu.no_stderr(r1)
}

# origin: uutils test_ls::test_ls_cf_output_should_be_delimited_by_tab
test test_uu_ls_ls_cf_output_should_be_delimited_by_tab { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "e")?
  uu.mkdir(s, "e/a2345")?
  uu.mkdir(s, "e/b")?
  let r1 = ls_run(s, ["-CF", "e"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a2345/\tb/\n")
}

# origin: uutils test_ls::test_ls_bad_ls_colors_is_an_error_not_a_warning
test test_uu_ls_ls_bad_ls_colors_is_an_error_not_a_warning { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "marker")?
  let r1 = ls_run(s, [])?
  let r2 = ls_run(s, [])?
}

# origin: uutils test_ls::test_a_overrides_f_files
test test_uu_ls_a_overrides_f_files { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "visible")?
  uu.touch(s, ".hidden")?
  let r1 = ls_run(s, ["-f", "-a"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, ".hidden")
  uu.stdout_contains(r1, "visible")
}

# origin: uutils test_ls::test_f_flag_enables_all
test test_uu_ls_f_flag_enables_all { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "visible")?
  uu.touch(s, ".hidden")?
  let r1 = ls_run(s, ["-f"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "visible")
  uu.stdout_contains(r1, ".hidden")
}

# origin: uutils test_ls::test_ls_block_size_override_self
test test_uu_ls_ls_block_size_override_self { |ctx|
  let s = uu.scene(ctx)?
  let r1 = ls_run(s, ["--block-size=512", "--block-size=512"])?
  uu.succeeds(r1)
  let r2 = ls_run(s, ["--human-readable", "--human-readable"])?
  uu.succeeds(r2)
  let r3 = ls_run(s, ["--si", "--si"])?
  uu.succeeds(r3)
}

# origin: uutils test_ls::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_ls_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r1 = pipe_listing_diagnostic(s, ["--block-size=1fb"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_is(r1, "ls: invalid suffix in --block-size argument '1fb'\n")
}

# origin: uutils test_ls::ls_emoji_alignment
test test_uu_ls_ls_emoji_alignment { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "💐", "漢"] { uu.touch(s, name)? }
  let r = ls_run(s, [])?
  uu.succeeds(r)
  uu.stdout_contains(r, "a")
  uu.stdout_contains(r, "💐")
  uu.stdout_contains(r, "漢")
}

# origin: uutils test_ls::test_invalid_utf8
test test_uu_ls_invalid_utf8 { |ctx|
  let s = uu.scene(ctx)?
  let file = uu.at_bytes(s, b"-\xe0-foo")?
  file.write(b"")?
  let r = ls_run(s, [])?
  uu.succeeds(r)
}

# origin: uutils test_ls::test_invalid_value_returns_1
test test_uu_ls_invalid_value_returns_1 { |ctx|
  let s = uu.scene(ctx)?
  for flag in ["--classify", "--color", "--format", "--hyperlink", "--indicator-style", "--quoting-style", "--sort", "--time"] {
    let r = ls_run(s, [f"{flag}=definitely_invalid_value"])?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
  }
}

# origin: uutils test_ls::test_invalid_value_returns_2
test test_uu_ls_invalid_value_returns_2 { |ctx|
  let s = uu.scene(ctx)?
  for flag in ["--block-size", "--width", "--tab-size"] {
    let r = ls_run(s, [f"{flag}=definitely_invalid_value"])?
    uu.fails_with_code(r, 2)
    uu.no_stdout(r)
  }
}

# origin: uutils test_ls::test_ls_al_no_capabilities_insufficient_on_wasi
test test_uu_ls_ls_al_no_capabilities_insufficient_on_wasi { |ctx|
  let s = uu.scene(ctx)?
  let r = ls_run(s, ["-al"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert "Capabilities insufficient" not in r.stdout.utf8()?
}

# origin: uutils test_ls::quoting::test_c_dot_utf8_renders_utf8
test test_uu_ls_quoting_c_dot_utf8_renders_utf8 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "é")?
  let r = ls_run(s, ["--quoting-style=literal", "--hide-control-chars"], vars: {LC_ALL: "C"})?
  uu.succeeds(r)
  uu.stdout_is(r, "??\n")
  let utf8 = ls_run(s, ["--quoting-style=literal", "--hide-control-chars"], vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(utf8)
  uu.stdout_is(utf8, "é\n")
}

# origin: uutils test_ls::test_f_flag_disables_sorting
test test_uu_ls_f_flag_disables_sorting { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "zebra", "")?
  uu.write(s, "apple", "")?
  uu.write(s, "banana", "")?
  let r0 = ls_run(s, ["-f", "-1"])?
  uu.succeeds(r0)
  let r1 = ls_run(s, ["-U", "-a", "-1"])?
  uu.succeeds(r1)
  let r2 = ls_run(s, ["-f", "--sort=name", "-1"])?
  uu.succeeds(r2)
  let r3 = ls_run(s, ["-a", "-1"])?
  uu.succeeds(r3)
  assert r0.stdout == r1.stdout
  assert r2.stdout == r3.stdout
}

# origin: uutils test_ls::test_f_overrides_sort_flags
test test_uu_ls_f_overrides_sort_flags { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "small.txt", "a")?
  uu.write(s, "medium.txt", "bb")?
  uu.write(s, "large.txt", "ccc")?
  let r0 = ls_run(s, ["-S", "-a", "-1"])?
  uu.succeeds(r0)
  let r1 = ls_run(s, ["-U", "-a", "-1"])?
  uu.succeeds(r1)
  let r2 = ls_run(s, ["-f", "-S", "-1"])?
  uu.succeeds(r2)
  let r3 = ls_run(s, ["-S", "-f", "-1"])?
  uu.succeeds(r3)
  assert r2.stdout == r0.stdout
  assert r3.stdout == r1.stdout
}

# origin: uutils test_ls::test_big_u_participates_in_sort_flag_wins
test test_uu_ls_big_u_participates_in_sort_flag_wins { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "small.txt", "a")?
  uu.write(s, "medium.txt", "bb")?
  uu.write(s, "large.txt", "ccc")?
  let r0 = ls_run(s, ["-S", "-1"])?
  uu.succeeds(r0)
  let r1 = ls_run(s, ["-U", "-1"])?
  uu.succeeds(r1)
  let r2 = ls_run(s, ["-U", "-S", "-1"])?
  uu.succeeds(r2)
  let r3 = ls_run(s, ["-S", "-U", "-1"])?
  uu.succeeds(r3)
  assert r2.stdout == r0.stdout
  assert r3.stdout == r1.stdout
  assert r0.stdout.utf8()?.lines()[..3] == ["large.txt", "medium.txt", "small.txt"]
}

# origin: uutils test_ls::test_f_overrides_big_a
test test_uu_ls_f_overrides_big_a { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "visible")?
  uu.touch(s, ".hidden")?
  let af = ls_run(s, ["-A", "-f", "-1"])?
  uu.succeeds(af)
  assert ".." in af.stdout.utf8()?.lines()
  let fa = ls_run(s, ["-f", "-A", "-1"])?
  uu.succeeds(fa)
  assert ".." not in fa.stdout.utf8()?.lines()
}

# origin: uutils test_ls::test_f_flag_disables_implicit_color
test test_uu_ls_f_flag_disables_implicit_color { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "file.txt")?
  let r = ls_run(s, ["-f"])?
  uu.succeeds(r)
  assert "\u{1b}[" not in r.stdout.utf8()?
}

# origin: uutils test_ls::test_explicit_color_always_works_with_f
test test_uu_ls_explicit_color_always_works_with_f { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "file.txt")?
  let first = ls_run(s, ["--color=always", "-f"])?
  uu.succeeds(first)
  assert "\u{1b}[" not in first.stdout.utf8()?
  let second = ls_run(s, ["-f", "--color=always"])?
  uu.succeeds(second)
  assert "\u{1b}[" not in second.stdout.utf8()?
}

# origin: uutils test_ls::test_f_flag_combined_behavior
test test_uu_ls_f_flag_combined_behavior { |ctx|
  let s = uu.scene(ctx)?
  for name in ["zebra.txt", ".hidden", "apple.txt"] { uu.touch(s, name)? }
  uu.mkdir(s, "directory")?
  let r = ls_run(s, ["-f"])?
  uu.succeeds(r)
  uu.stdout_contains(r, ".hidden")
  uu.stdout_contains(r, "zebra.txt")
  uu.stdout_contains(r, "apple.txt")
  uu.stdout_contains(r, "directory")
  assert "\u{1b}[" not in r.stdout.utf8()?
}

# origin: uutils test_ls::test_f_with_long_format
test test_uu_ls_f_with_long_format { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file1")?
  uu.touch(s, ".hidden")?
  let r = ls_run(s, ["-f", "-l"])?
  uu.succeeds(r)
  uu.stdout_contains(r, ".hidden")
  uu.stdout_contains(r, "file1")
  uu.stdout_contains(r, "-rw")
}

# origin: uutils test_ls::test_ls_across
test test_uu_ls_ls_across { |ctx|
  let s = uu.scene(ctx)?
  for i in range(1, 5) { uu.touch(s, f"test-across-{i}")? }
  for option in ["-x", "--format=across", "--format=acr", "--format=horizontal", "--for=across", "--for=horizontal"] {
    let plain = ls_run(s, [option])?
    uu.succeeds(plain)
    uu.stdout_only(plain, "test-across-1  test-across-2  test-across-3  test-across-4\n")
    let width = ls_run(s, ["-w=30", option])?
    uu.fails_with_code(width, 2)
    uu.no_stdout(width)
    uu.stderr_is(width, "ls: invalid line width: '=30'\n")
  }
}

# origin: uutils test_ls::test_ls_columns
test test_uu_ls_ls_columns { |ctx|
  let s = uu.scene(ctx)?
  for i in range(1, 5) { uu.touch(s, f"test-columns-{i}")? }
  let default = ls_run(s, [])?
  uu.succeeds(default)
  uu.stdout_only(default, "test-columns-1\ntest-columns-2\ntest-columns-3\ntest-columns-4\n")
  for option in ["-C", "--format=columns", "--for=columns"] {
    let plain = ls_run(s, [option])?
    if option == "-C" {
      uu.succeeds(plain)
      uu.stdout_only(plain, "test-columns-1\ttest-columns-2\ttest-columns-3\ttest-columns-4\n")
    } else {
      uu.fails_with_code(plain, 1)
      uu.no_stdout(plain)
      uu.stderr_is(plain, "ls: invalid argument 'columns' for '--format'\nValid arguments are:\n  - 'verbose', 'long'\n  - 'commas'\n  - 'horizontal', 'across'\n  - 'vertical'\n  - 'single-column'\nTry 'ls --help' for more information.\n")
    }
    let width = ls_run(s, ["-w=40", option])?
    uu.fails_with_code(width, 2)
    uu.no_stdout(width)
    uu.stderr_is(width, "ls: invalid line width: '=40'\n")
    let environment = ls_run(s, [option], vars: {COLUMNS: "40"})?
    if option == "-C" {
      uu.succeeds(environment)
      uu.stdout_only(environment, "test-columns-1\ttest-columns-3\ntest-columns-2\ttest-columns-4\n")
    } else {
      uu.fails_with_code(environment, 1)
      uu.no_stdout(environment)
      uu.stderr_is(environment, "ls: invalid argument 'columns' for '--format'\nValid arguments are:\n  - 'verbose', 'long'\n  - 'commas'\n  - 'horizontal', 'across'\n  - 'vertical'\n  - 'single-column'\nTry 'ls --help' for more information.\n")
    }
  }
  let bad = ls_run(s, ["-C"], vars: {COLUMNS: "garbage"})?
  uu.succeeds(bad)
  uu.stdout_is(bad, "test-columns-1\ttest-columns-2\ttest-columns-3\ttest-columns-4\n")
  uu.stderr_is(bad, "ls: ignoring invalid width in environment variable COLUMNS: 'garbage'\n")
  let zero = ls_run(s, ["-Cw0"])?
  uu.succeeds(zero)
  uu.stdout_only(zero, "test-columns-1  test-columns-2  test-columns-3  test-columns-4\n")
  let comma = ls_run(s, ["-mw0"])?
  uu.succeeds(comma)
  uu.stdout_only(comma, "test-columns-1, test-columns-2, test-columns-3, test-columns-4\n")
}

# origin: uutils test_ls::test_ls_commas
test test_uu_ls_ls_commas { |ctx|
  let s = uu.scene(ctx)?
  for i in range(1, 5) { uu.touch(s, f"test-commas-{i}")? }
  for option in ["-m", "--format=commas", "--for=commas"] {
  let plain = ls_run(s, [option])?
  uu.succeeds(plain)
  uu.stdout_only(plain, "test-commas-1, test-commas-2, test-commas-3, test-commas-4\n")
  let width30 = ls_run(s, ["-w=30", option])?
  uu.fails_with_code(width30, 2)
  uu.no_stdout(width30)
  uu.stderr_is(width30, "ls: invalid line width: '=30'\n")
  let width45 = ls_run(s, ["-w=45", option])?
  uu.fails_with_code(width45, 2)
  uu.no_stdout(width45)
  uu.stderr_is(width45, "ls: invalid line width: '=45'\n")
  }
  for name in ["a", "bb", "c", "com,ma", "n\nl"] { uu.touch(s, name)? }
  let r0 = ls_run(s, ["-m", "-w5", "a", "bb"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "a, bb\n")
  let r1 = ls_run(s, ["-m", "-w5", "a", "bb", "c"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a,\nbb, c\n")
  let r2 = ls_run(s, ["-m", "--quoting-style=shell", "com,ma"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "'com,ma'\n")
  let r3 = ls_run(s, ["-m", "--quoting-style=escape", "com,ma"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "com\\,ma\n")
  let r4 = ls_run(s, ["-m", "n\nl"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "n?l\n")
  let r5 = ls_run(s, ["-m", "--show-control-chars", "n\nl"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "n\nl\n")
}

# origin: uutils test_ls::test_ls_commas_trailing
test test_uu_ls_ls_commas_trailing { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test-commas-trailing-2")?
  uu.touch(s, "test-commas-trailing-1")?
  uu.append(s, "test-commas-trailing-1", [f"{i}" for i in range(2000)].join("\n"))?
  let r = ls_run(s, ["-sm", "./test-commas-trailing-1", "./test-commas-trailing-2"])?
  uu.succeeds(r)
  assert rx"\S\n$".matches(r.stdout.utf8()?)
}

# origin: uutils test_ls::test_ls_block_size_override
test test_uu_ls_ls_block_size_override { |ctx|
  let s = uu.scene(ctx)?
  let dd = uu.invoke(s, "dd", ["if=/dev/zero", "of=file", "bs=1024", "count=1"])?
  uu.succeeds(dd)
  let r0 = ls_run(s, ["-s", "--block-size=512", "--si"])?
  uu.succeeds(r0)
  assert "total 4.1k" in r0.stdout.utf8()?.lines()
  let r1 = ls_run(s, ["-s", "--si", "--block-size=512"])?
  uu.succeeds(r1)
  assert "total 8" in r1.stdout.utf8()?.lines()
  let r2 = ls_run(s, ["-s", "--block-size=512", "--human-readable"])?
  uu.succeeds(r2)
  assert "total 4.0K" in r2.stdout.utf8()?.lines()
  let r3 = ls_run(s, ["-s", "--human-readable", "--block-size=512"])?
  uu.succeeds(r3)
  assert "total 8" in r3.stdout.utf8()?.lines()
}

# origin: uutils test_ls::test_ls_block_size_si_file_size
test test_uu_ls_ls_block_size_si_file_size { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "file", bytes.zero(1024)?)?
  let r0 = ls_run(s, ["-l", "--block-size=512", "--si"])?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "1.1k")
  let r1 = ls_run(s, ["-l", "--si", "--block-size=512"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, " 2 ")
  let r2 = ls_run(s, ["-l", "--block-size=512", "--human-readable"])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "1.0K")
  let r3 = ls_run(s, ["-l", "--human-readable", "--block-size=512"])?
  uu.succeeds(r3)
  uu.stdout_contains(r3, " 2 ")
}

# origin: uutils test_ls::quoting::test_locale_aware_quoting
test test_uu_ls_quoting_locale_aware_quoting { |ctx|
  let s = uu.scene(ctx)?
  let scene0 = uu.scene(ctx)?
  let file0 = uu.at_bytes(scene0, b"\xf0\x9f\x98\x81")?
  file0.write(b"")?
  let ascii0 = ls_run(scene0, ["--quoting-style=shell-escape"], vars: {LC_ALL: "C"})?
  uu.succeeds(ascii0)
  uu.stdout_only(ascii0, r"""''$'\360\237\230\201'
""")
  let utf80 = ls_run(scene0, ["--quoting-style=shell-escape"], vars: {LC_ALL: "en_US.UTF-8"})?
  uu.succeeds(utf80)
  uu.stdout_only(utf80, "😁\n")
  let scene1 = uu.scene(ctx)?
  let file1 = uu.at_bytes(scene1, b"\xe2\x82\xac")?
  file1.write(b"")?
  let ascii1 = ls_run(scene1, ["--quoting-style=shell-escape"], vars: {LC_ALL: "C"})?
  uu.succeeds(ascii1)
  uu.stdout_only(ascii1, r"""''$'\342\202\254'
""")
  let utf81 = ls_run(scene1, ["--quoting-style=shell-escape"], vars: {LC_ALL: "en_US.UTF-8"})?
  uu.succeeds(utf81)
  uu.stdout_only(utf81, "€\n")
  let scene2 = uu.scene(ctx)?
  let file2 = uu.at_bytes(scene2, b"\xc2\x80\xc2\x81")?
  file2.write(b"")?
  let ascii2 = ls_run(scene2, ["--quoting-style=literal", "--hide-control-char"], vars: {LC_ALL: "C"})?
  uu.succeeds(ascii2)
  uu.stdout_only(ascii2, "????\n")
  let utf82 = ls_run(scene2, ["--quoting-style=literal", "--hide-control-char"], vars: {LC_ALL: "en_US.UTF-8"})?
  uu.succeeds(utf82)
  uu.stdout_only(utf82, "??\n")
  let scene3 = uu.scene(ctx)?
  let file3 = uu.at_bytes(scene3, b"\xc2\xc2\x81")?
  file3.write(b"")?
  let ascii3 = ls_run(scene3, ["--quoting-style=literal", "--hide-control-char"], vars: {LC_ALL: "C"})?
  uu.succeeds(ascii3)
  uu.stdout_only(ascii3, "???\n")
  let utf83 = ls_run(scene3, ["--quoting-style=literal", "--hide-control-char"], vars: {LC_ALL: "en_US.UTF-8"})?
  uu.succeeds(utf83)
  uu.stdout_only(utf83, "??\n")
  let scene4 = uu.scene(ctx)?
  let file4 = uu.at_bytes(scene4, b"\xc2\x81\xc2")?
  file4.write(b"")?
  let ascii4 = ls_run(scene4, ["--quoting-style=literal", "--hide-control-char"], vars: {LC_ALL: "C"})?
  uu.succeeds(ascii4)
  uu.stdout_only(ascii4, "???\n")
  let utf84 = ls_run(scene4, ["--quoting-style=literal", "--hide-control-char"], vars: {LC_ALL: "en_US.UTF-8"})?
  uu.succeeds(utf84)
  uu.stdout_only(utf84, "??\n")
}

# origin: uutils test_ls::quoting::test_ls_quoting_locale_utf8
test test_uu_ls_quoting_ls_quoting_locale_utf8 { |ctx|
  let s = uu.scene(ctx)?
  for name in ["hello world", "it's", "say \"hi\"", "tab\there", "nel\u{85}here"] { uu.touch(s, name)? }
  for style in ["locale", "clocale"] {
    let r = ls_run(s, [f"--quoting-style={style}", "-1"], vars: {LC_ALL: "en_US.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_contains(r, "‘hello world’")
    uu.stdout_contains(r, "‘it's’")
    uu.stdout_contains(r, "‘say \"hi\"’")
    uu.stdout_contains(r, "‘tab\\there’")
    uu.stdout_contains(r, "‘nel\\302\\205here’")
  }
  let c0 = uu.scene(ctx)?
  uu.touch(c0, "hello world")?
  let r0 = ls_run(c0, ["--quoting-style=locale", "-1"], vars: {LC_ALL: "C"})?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "'hello world'")
  let c1 = uu.scene(ctx)?
  uu.touch(c1, "hello world")?
  let r1 = ls_run(c1, ["--quoting-style=clocale", "-1"], vars: {LC_ALL: "C"})?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "\"hello world\"")
}

# origin: uutils test_ls::test_dired_write_error
test test_uu_ls_dired_write_error { |ctx|
  let s = uu.scene(ctx)?
  let r = ls_run(s, ["--dired", "nonexistent"], stdout: p"/dev/full")?
  uu.fails_with_code(r, 2)
  uu.stderr_is(r, "ls: cannot access 'nonexistent': No such file or directory\nls: write error: No space left on device\n")
}

# origin: uutils test_ls::test_ls_dangling_symlink_or_and_missing_colors
test test_uu_ls_ls_dangling_symlink_or_and_missing_colors { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, uu.at(s, "nowhere").display(), "dangling")?
  let r = ls_run(s, ["-o", "--time-style=+:TIME:", "--color=always", "dangling"], vars: {LS_COLORS: "ln=target:or=40:mi=34"})?
  uu.succeeds(r)
  let captures = rx"\x1b\[0m\x1b\[([0-9;]*)mdangling\x1b\[0m -> \x1b\[([0-9;]*)m".captures(r.stdout.utf8()?)
  assert captures.len() >= 3
  assert captures[1] == "40"
  assert captures[2] == "34"
}

# origin: uutils test_ls::test_ls_dangling_symlink_ln_or_priority
test test_uu_ls_ls_dangling_symlink_ln_or_priority { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, uu.at(s, "nowhere").display(), "dangling")?
  let r = ls_run(s, ["-o", "--time-style=+:TIME:", "--color=always", "dangling"], vars: {LS_COLORS: "ln=34:mi=35:or=36"})?
  uu.succeeds(r)
  let captures = rx"\x1b\[0m\x1b\[([0-9;]*)mdangling\x1b\[0m -> \x1b\[([0-9;]*)m".captures(r.stdout.utf8()?)
  assert captures.len() >= 3
  assert captures[1] == "36"
  assert captures[2] == "35"
}

# origin: uutils test_ls::test_ls_dangling_symlink_ln_and_missing_colors
test test_uu_ls_ls_dangling_symlink_ln_and_missing_colors { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, uu.at(s, "nowhere").display(), "dangling")?
  let r = ls_run(s, ["-o", "--time-style=+:TIME:", "--color=always", "dangling"], vars: {LS_COLORS: "ln=34:mi=35"})?
  uu.succeeds(r)
  let captures = rx"\x1b\[0m\x1b\[([0-9;]*)mdangling\x1b\[0m -> \x1b\[([0-9;]*)m".captures(r.stdout.utf8()?)
  assert captures.len() >= 3
  assert captures[1] == "34"
  assert captures[2] == "35"
}

# origin: uutils test_ls::test_ls_dangling_symlink_blank_or_still_emits_reset
test test_uu_ls_ls_dangling_symlink_blank_or_still_emits_reset { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, uu.at(s, "nowhere").display(), "dangling")?
  let r = ls_run(s, ["--color=always", "dangling"], vars: {LS_COLORS: "ln=target:or=:ex=:"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "\u{1b}[0m\u{1b}[mdangling\u{1b}[0m")
}

# origin: uutils test_ls::test_ls_dangling_symlink_blank_or_in_directory_listing
test test_uu_ls_ls_dangling_symlink_blank_or_in_directory_listing { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.symlink(s, uu.at(s, "nowhere").display(), "dir/entry")?
  let r = ls_run(s, ["--color=always", "dir"], vars: {LS_COLORS: "ln=target:or=:ex=:"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "\u{1b}[0m\u{1b}[mentry\u{1b}[0m")
}

# origin: uutils test_ls::test_ls_dangling_symlink_uses_ln_when_or_blank
test test_uu_ls_ls_dangling_symlink_uses_ln_when_or_blank { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, uu.at(s, "nowhere").display(), "dangling")?
  let r = ls_run(s, ["--color=always", "dangling"], vars: {LS_COLORS: "ln=1;36:or=:"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "\u{1b}[0m\u{1b}[1;36mdangling\u{1b}[0m")
}

# origin: uutils test_ls::test_dereference_symlink_dir_color
test test_uu_ls_dereference_symlink_dir_color { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.mkdir(s, "dir1/link")?
  let expected = ls_run(s, ["--color", "dir1"])?
  uu.succeeds(expected)
  let linked = uu.scene(ctx)?
  uu.mkdir(linked, "dir1")?
  uu.mkdir(linked, "dir2")?
  uu.symlink(linked, "../dir2", "dir1/link")?
  let actual = ls_run(linked, ["-L", "--color", "dir1"])?
  uu.succeeds(actual)
  assert actual.stdout == expected.stdout
}

# origin: uutils test_ls::test_dereference_symlink_file_color
test test_uu_ls_dereference_symlink_file_color { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.touch(s, "dir1/link")?
  let expected = ls_run(s, ["--color", "dir1"])?
  uu.succeeds(expected)
  let linked = uu.scene(ctx)?
  uu.mkdir(linked, "dir1")?
  uu.touch(linked, "file")?
  uu.symlink(linked, "../file", "dir1/link")?
  let actual = ls_run(linked, ["-L", "--color", "dir1"])?
  uu.succeeds(actual)
  assert actual.stdout == expected.stdout
}

# origin: uutils test_ls::test_dereference_dangling_color
test test_uu_ls_dereference_dangling_color { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "wat", "nonexistent")?
  let expected = ls_run(s, ["--color"])?
  uu.succeeds(expected)
  let linked = uu.scene(ctx)?
  uu.symlink(linked, "wat", "nonexistent")?
  let actual = ls_run(linked, ["-L", "--color"])?
  uu.succeeds(actual)
  uu.no_stderr(actual)
  assert actual.stdout == expected.stdout
}

# origin: uutils test_ls::test_ls_deref
test test_uu_ls_ls_deref { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test-long")?
  uu.symlink(s, uu.at(s, "test-long").display(), "test-long.link")?
  assert uu.is_symlink(s, "test-long.link")?
  for color in ["--color=never", "--color=neve", "--color=n"] {
    let r = ls_run(s, ["-l", color, "test-long", "test-long.link"])?
    uu.succeeds(r)
    assert rx"(.*)test-long.link -> (.*)test-long(.*)".matches(r.stdout.utf8()?.trim())
  }
  let dereferenced = ls_run(s, ["-L", "--color=never", "test-long", "test-long.link"])?
  uu.succeeds(dereferenced)
  assert ! rx"(.*)test-long.link -> (.*)test-long(.*)".matches(dereferenced.stdout.utf8()?.trim())
}

# origin: uutils test_ls::test_ls_deref_command_line
test test_uu_ls_ls_deref_command_line { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "some_file")?
  uu.symlink(s, uu.at(s, "some_file").display(), "sym_file")?
  let r0 = ls_run(s, ["sym_file"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "sym_file\n")
  let r1 = ls_run(s, ["-l", "sym_file"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "sym_file ->")
  let r2 = ls_run(s, ["--dereference-command-line-symlink-to-dir", "sym_file"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "sym_file\n")
  let r3 = ls_run(s, ["-l", "--dereference-command-line-symlink-to-dir", "sym_file"])?
  uu.succeeds(r3)
  uu.stdout_contains(r3, "sym_file ->")
  let r4 = ls_run(s, ["--dereference-command-line", "sym_file"])?
  uu.succeeds(r4)
  uu.stdout_is(r4, "sym_file\n")
  let r5 = ls_run(s, ["-l", "--dereference-command-line", "sym_file"])?
  uu.succeeds(r5)
  assert "->" not in r5.stdout.utf8()?
  let r6 = ls_run(s, ["-lH", "sym_file"])?
  uu.succeeds(r6)
  assert "sym_file ->" not in r6.stdout.utf8()?
  let r7 = ls_run(s, ["-l", "--dereference-command-line"])?
  uu.succeeds(r7)
  uu.stdout_contains(r7, "sym_file ->")
}

# origin: uutils test_ls::test_ls_color_do_not_reset
test test_uu_ls_ls_color_do_not_reset { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "example")?
  uu.mkdir(s, "example/a")?
  uu.mkdir(s, "example/b")?
  let r = ls_run(s, ["--color=always", "example/"])?
  uu.succeeds(r)
  uu.stdout_is(r, "a\nb\n")
}

# origin: uutils test_ls::test_ls_color_does_not_make_quoted_names_align_as_unquoted
test test_uu_ls_ls_color_does_not_make_quoted_names_align_as_unquoted { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir one")?
  uu.touch(s, "file one")?
  for args in [["-C", "-T0", "-w=80", "--quoting-style=shell-escape-always"], ["-l", "--quoting-style=shell-escape-always"]] {
    let plain = ls_run(s, args.extend(["--color=never"]))?
    if "-w=80" in args {
      uu.fails_with_code(plain, 2)
      uu.no_stdout(plain)
      uu.stderr_is(plain, "ls: invalid line width: '=80'\n")
    } else { uu.succeeds(plain) }
    let colored = ls_run(s, args.extend(["--color=always"]))?
    if "-w=80" in args {
      uu.fails_with_code(colored, 2)
      uu.no_stdout(colored)
      uu.stderr_is(colored, "ls: invalid line width: '=80'\n")
    } else { uu.succeeds(colored) }
    assert rx"\x1b\[[0-9;]*[A-Za-z]".replace(colored.stdout.utf8()?, with: "") == plain.stdout.utf8()?
  }
}

# Linux ACL v2 fixture fields reproduce the default/access entries installed by setfacl.
proc acl_fixture_entry(tag: Int, permissions: Int, id: Int = 4294967295) [error] -> Result[Bytes, Error] {
  Ok(bytes.concat([bytes.pack_le(tag, 2)?, bytes.pack_le(permissions, 2)?, bytes.pack_le(id, 4)?]))
}

proc default_acl_fixture(target: Path, named_bin: Bool = false) [fs, error] -> Result[Unit, Error] {
  let mode = fs.stat(target)?.mode
  var entries = [b"\x02\0\0\0", acl_fixture_entry(1, mode / 64 % 8)?]
  if named_bin { entries += [acl_fixture_entry(2, 7, user.lookup("bin")?.uid)?] }
  entries += [acl_fixture_entry(4, if named_bin { mode / 8 % 8 } else { 7 })?]
  if named_bin { entries += [acl_fixture_entry(16, 7)?] }
  entries += [acl_fixture_entry(32, mode % 8)?]
  fs.xattr_set(target, "system.posix_acl_default", bytes.concat(entries))?
  Ok()
}

proc access_acl_fixture(target: Path, uid: Int) [fs, error] -> Result[Unit, Error] {
  let mode = fs.stat(target)?.mode
  let data = bytes.concat([b"\x02\0\0\0", acl_fixture_entry(1, mode / 64 % 8)?, acl_fixture_entry(2, 6, uid)?,
    acl_fixture_entry(4, mode / 8 % 8)?, acl_fixture_entry(16, (mode / 8 % 8).bit_or(6))?, acl_fixture_entry(32, mode % 8)?])
  fs.xattr_set(target, "system.posix_acl_access", data)?
  Ok()
}

# origin: uutils test_ls::test_acl_display
test test_uu_ls_acl_display { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "with_acl")?
  uu.mkdir(s, "without_acl")?
  default_acl_fixture(uu.at(s, "with_acl"))?
  let first = ls_run(s, ["-la", s.root.display()])?
  uu.succeeds(first)
  assert rx"[a-z-]*\+ .*with_acl".matches(first.stdout.utf8()?)
  assert rx"[a-z-]* .*without_acl".matches(first.stdout.utf8()?)
  let moved = ls_run({...s, root: p"/"}, ["-la", s.root.display()])?
  uu.succeeds(moved)
  assert rx"[a-z-]*\+ .*with_acl".matches(moved.stdout.utf8()?)
  assert rx"[a-z-]* .*without_acl".matches(moved.stdout.utf8()?)
}

# origin: uutils test_ls::test_acl_display_symlink
test test_uu_ls_acl_display_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  default_acl_fixture(uu.at(s, "dir"), named_bin: true)?
  uu.symlink(s, uu.at(s, "dir").display(), "link")?
  let r = ls_run(s, ["-lLd", "link"])?
  uu.succeeds(r)
  assert rx"[a-z-]*\+\s\d+\s.*link".matches(r.stdout.utf8()?)
  let listed = ls_run(s, ["-l"])?
  uu.succeeds(listed)
  let columns = [rx"^[^0-9]*".captures(line)[0].byte_len() for line in listed.stdout.utf8()?.lines()[1..] if rx"[0-9]".matches(line)]
  assert ! columns.is_empty()
  for column in columns { assert column == columns[0] }
}

# origin: uutils test_ls::test_acl_display_symlink_without_dereference
test test_uu_ls_acl_display_symlink_without_dereference { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  default_acl_fixture(uu.at(s, "dir"), named_bin: true)?
  uu.symlink(s, uu.at(s, "dir").display(), "link")?
  let r = ls_run(s, ["-ld", "link"])?
  uu.succeeds(r)
  assert "+" not in r.stdout.utf8()?
}

# origin: uutils test_ls::test_acl_padding_not_inflated
test test_uu_ls_acl_padding_not_inflated { |ctx|
  let s = uu.scene(ctx)?
  let uid = unix.id()?.uid
  for name in ["file1", "file2", "file3", "file4", "file5"] {
    uu.touch(s, name)?
    access_acl_fixture(uu.at(s, name), uid)?
  }
  let r = ls_run(s, ["-l"])?
  uu.succeeds(r)
  var checked = 0
  for line in r.stdout.utf8()?.lines() {
    if line.starts_with("-") and "+" in line {
      let after_plus = line.split("+")[1]
      let leading_spaces = rx"^ *".captures(after_plus)[0].byte_len()
      assert leading_spaces == 1
      checked += 1
    }
  }
  assert checked > 0
}

# origin: uutils test_ls::test_device_number
test test_uu_ls_device_number { |ctx|
  let s = uu.scene(ctx)?
  let devices = [candidate for candidate in p"/dev".glob("*")? if fs.stat(candidate)?.kind in ["char", "block"]]
  assert ! devices.is_empty()
  let device = devices[0]
  let number = fs.stat(device, follow_symlinks: true)?.rdev
  let r = ls_run(s, ["-l", device.display()])?
  uu.succeeds(r)
  uu.stdout_contains(r, f"{fs.dev_major(number)}, {fs.dev_minor(number)}")
}

# origin: uutils test_ls::test_ls_a
test test_uu_ls_ls_a { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, ".test-1")?
  uu.mkdir(s, "some-dir")?
  uu.touch(s, "some-dir/.test-2")?
  let r0 = ls_run(s, ["-1"])?
  uu.succeeds(r0)
  assert ".test-1" not in r0.stdout.utf8()?
  assert ".." not in r0.stdout.utf8()?
  assert ! rx"^\.\n".matches(r0.stdout.utf8()?)
  let r1 = ls_run(s, ["-a", "-1"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, ".test-1")
  uu.stdout_contains(r1, "..")
  assert rx"^\.\n".matches(r1.stdout.utf8()?)
  let r2 = ls_run(s, ["-A", "-1"])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, ".test-1")
  assert ".." not in r2.stdout.utf8()?
  assert ! rx"^\.\n".matches(r2.stdout.utf8()?)
  let r3 = ls_run(s, ["-1", "some-dir"])?
  uu.succeeds(r3)
  assert ".test-2" not in r3.stdout.utf8()?
  assert ".." not in r3.stdout.utf8()?
  assert ! rx"^\.\n".matches(r3.stdout.utf8()?)
  let r4 = ls_run(s, ["-a", "-1", "some-dir"])?
  uu.succeeds(r4)
  uu.stdout_contains(r4, ".test-2")
  uu.stdout_contains(r4, "..")
  uu.no_stderr(r4)
  assert rx"^\.\n".matches(r4.stdout.utf8()?)
  let r5 = ls_run(s, ["-A", "-1", "some-dir"])?
  uu.succeeds(r5)
  uu.stdout_contains(r5, ".test-2")
  assert ".." not in r5.stdout.utf8()?
  assert ! rx"^\.\n".matches(r5.stdout.utf8()?)
}

# origin: uutils test_ls::test_ls_align_unquoted
test test_uu_ls_ls_align_unquoted { |ctx|
  let s = uu.scene(ctx)?
  for name in ["elf two", "foobar", "CAPS", "'quoted'"] { uu.touch(s, name)? }
  let tty = terminal_listing(s, ["--color"])?
  uu.succeeds(tty)
  uu.stdout_only(tty, "\"'quoted'\"   CAPS  'elf two'   foobar\r\n")
  for format in ["--format=column", "--format=across"] {
    let r = ls_run(s, ["--color", format, "--quoting-style=shell"])?
    if format == "--format=column" {
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_is(r, "ls: invalid argument 'column' for '--format'\nValid arguments are:\n  - 'verbose', 'long'\n  - 'commas'\n  - 'horizontal', 'across'\n  - 'vertical'\n  - 'single-column'\nTry 'ls --help' for more information.\n")
    } else {
      uu.succeeds(r)
      uu.stdout_only(r, "\"'quoted'\"   CAPS  'elf two'   foobar\n")
    }
  }
}

# origin: uutils test_ls::test_ls_dangling_symlinks
test test_uu_ls_ls_dangling_symlinks { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "temp_dir")?
  uu.symlink(s, uu.at(s, "does_not_exist").display(), "temp_dir/dangle")?
  for option in ["-L", "-H"] {
    let r = ls_run(s, [option, "temp_dir/dangle"])?
    uu.fails_with_code(r, 2)
  }
  let plain = ls_run(s, ["temp_dir/dangle"])?
  uu.succeeds(plain)
  uu.stdout_contains(plain, "dangle")
  for option in ["-Li", "-LZ"] {
    let r = ls_run(s, [option, "temp_dir"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cannot access")
    uu.stderr_contains(r, "No such file or directory")
    uu.stdout_contains(r, "? dangle")
  }
  let long = ls_run(s, ["-Ll", "temp_dir"])?
  uu.fails_with_code(long, 1)
  uu.stdout_contains(long, "l?????????")
  uu.touch(s, "temp_dir/real_file")?
  let real = ls_run(s, ["-Li1", "temp_dir"])?
  uu.fails_with_code(real, 1)
  let real_line = real.stdout.utf8()?.lines()[1]
  assert real_line.ends_with("real_file")
  let real_length = real_line.byte_len() - "real_file".byte_len()
  let dangling = ls_run(s, ["-Li1", "temp_dir"])?
  uu.fails_with_code(dangling, 1)
  let dangling_line = dangling.stdout.utf8()?.lines()[0]
  assert dangling_line.ends_with("dangle")
  let dangling_length = dangling_line.byte_len() - "dangle".byte_len()
  assert real_length == dangling_length
}

# origin: uutils test_ls::test_ls_color
test test_uu_ls_ls_color { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "a/nested_dir")?
  uu.mkdir(s, "z")?
  uu.touch(s, "a/nested_file")?
  uu.touch(s, "test-color")?
  let default = ls_run(s, [])?
  uu.succeeds(default)
  assert "\u{1b}[0m\u{1b}[01;34ma\u{1b}[0m" not in default.stdout.utf8()?
  assert "\u{1b}[01;34mz\u{1b}[0m\n" not in default.stdout.utf8()?
  for param in ["--color", "--col", "--color=always", "--col=always"] {
  let r = ls_run(s, [param])?
  uu.succeeds(r)
  uu.stdout_is(r, "a\ntest-color\nz\n")
  }
  let never = ls_run(s, ["--color=never"])?
  uu.succeeds(never)
  assert "\u{1b}[0m\u{1b}[01;34ma\u{1b}[0m" not in never.stdout.utf8()?
  assert "\u{1b}[01;34mz\u{1b}[0m\n" not in never.stdout.utf8()?
  let nested = ls_run(s, ["--color", "a"])?
  uu.succeeds(nested)
  uu.stdout_is(nested, "nested_dir\nnested_file\n")
  let empty = ls_run(s, ["--color=never", "z"])?
  uu.succeeds(empty)
  uu.stdout_only(empty, "")
  uu.touch(s, "b")?
  let grid = ls_run(s, ["--color", "-w=15", "-C"])?
  uu.fails_with_code(grid, 2)
  uu.no_stdout(grid)
  uu.stderr_is(grid, "ls: invalid line width: '=15'\n")
}

# origin: uutils test_ls::test_ls_allocation_size
test test_uu_ls_ls_allocation_size { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "some-dir1")?
  uu.touch(s, "some-dir1/empty-file")?
  let truncate = uu.invoke(s, "truncate", ["-s", "4M", "some-dir1/file-with-holes"])?
  uu.succeeds(truncate)
  let zeros = uu.invoke(s, "dd", ["if=/dev/zero", "of=some-dir1/zero-file", "bs=1024", "count=4096"])?
  uu.succeeds(zeros)
  let irregular = uu.invoke(s, "dd", ["if=/dev/zero", "of=irregular-file", "bs=1", "count=777"])?
  uu.succeeds(irregular)
  let scaled = ls_run(s, ["-l", "--block-size=512", "irregular-file"])?
  uu.succeeds(scaled)
  assert rx"[^ ] 2 [^ ]".matches(scaled.stdout.utf8()?)
  let filesystem = uu.invoke(s, "df", ["-PT", s.root.display()])?
  uu.succeeds(filesystem)
  let captures = rx"Filesystem\s+Type\s+.+[\r\n]+([^\s]+)\s+([^\s]+)\s+".captures(filesystem.stdout.utf8()?)
  assert captures.len() >= 3
  let f2fs = captures[2] == "f2fs"
  let size4k = if f2fs { 4100 } else { 4096 }
  let size1k = if f2fs { 1025 } else { 1024 }
  let size8k = if f2fs { 8200 } else { 8192 }
  let size4m = if f2fs { "4.1M" } else { "4.0M" }
  let plain = ls_run(s, ["-s1", "some-dir1"])?
  uu.succeeds(plain)
  uu.stdout_is(plain, f"total {size4k}\n   0 empty-file\n   0 file-with-holes\n{size4k} zero-file\n")
  let long = ls_run(s, ["-sl", "some-dir1"])?
  uu.succeeds(long)
  uu.stdout_contains(long, "4194304")
  let sizes = ls_run(s, ["-s1", "some-dir1"])?
  uu.succeeds(sizes)
  uu.stdout_contains(sizes, "0 empty-file")
  uu.stdout_contains(sizes, f"{size4k} zero-file")
  let inodes = ls_run(s, ["-si1", "some-dir1"])?
  uu.succeeds(inodes)
  let lines = inodes.stdout.utf8()?.lines()
  assert lines[1].ends_with("empty-file")
  assert lines[2].ends_with("file-with-holes")
  assert lines[1].byte_len() - "empty-file".byte_len() == lines[2].byte_len() - "file-with-holes".byte_len()
  let r0 = ls_run(s, ["-s1", "some-dir1"], vars: {LS_BLOCK_SIZE: "8K", BLOCK_SIZE: "4K"})?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "total 512")
  uu.stdout_contains(r0, "0 empty-file")
  uu.stdout_contains(r0, "0 file-with-holes")
  uu.stdout_contains(r0, "512 zero-file")
  let r1 = ls_run(s, ["-s1", "some-dir1"], vars: {BLOCK_SIZE: "4K"})?
  uu.succeeds(r1)
  uu.stdout_contains(r1, f"total {size1k}")
  uu.stdout_contains(r1, "0 empty-file")
  uu.stdout_contains(r1, "0 file-with-holes")
  uu.stdout_contains(r1, f"{size1k} zero-file")
  let r2 = ls_run(s, ["-s1", "--si", "some-dir1"], vars: {BLOCK_SIZE: "4K"})?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "total 4.2M")
  uu.stdout_contains(r2, "0 empty-file")
  uu.stdout_contains(r2, "0 file-with-holes")
  uu.stdout_contains(r2, "4.2M zero-file")
  let r3 = ls_run(s, ["-s1", "some-dir1"], vars: {BLOCK_SIZE: "4096"})?
  uu.succeeds(r3)
  uu.stdout_contains(r3, f"total {size1k}")
  uu.stdout_contains(r3, "0 empty-file")
  uu.stdout_contains(r3, "0 file-with-holes")
  uu.stdout_contains(r3, f"{size1k} zero-file")
  let r4 = ls_run(s, ["-s1", "some-dir1"], vars: {POSIXLY_CORRECT: "true"})?
  uu.succeeds(r4)
  uu.stdout_contains(r4, f"total {size8k}")
  uu.stdout_contains(r4, "0 empty-file")
  uu.stdout_contains(r4, "0 file-with-holes")
  uu.stdout_contains(r4, f"{size8k} zero-file")
  let r5 = ls_run(s, ["-s1k", "some-dir1"], vars: {BLOCK_SIZE: "4K"})?
  uu.succeeds(r5)
  uu.stdout_contains(r5, f"total {size4k}")
  uu.stdout_contains(r5, "0 empty-file")
  uu.stdout_contains(r5, "0 file-with-holes")
  uu.stdout_contains(r5, f"{size4k} zero-file")
  let r6 = ls_run(s, ["-s1k", "--block-size=4K", "some-dir1"], vars: {})?
  uu.succeeds(r6)
  uu.stdout_contains(r6, f"total {size1k}")
  uu.stdout_contains(r6, "0 empty-file")
  uu.stdout_contains(r6, "0 file-with-holes")
  uu.stdout_contains(r6, f"{size1k} zero-file")
  let r7 = ls_run(s, ["-s1", "--block-size=4K", "some-dir1"], vars: {})?
  uu.succeeds(r7)
  uu.stdout_contains(r7, f"total {size1k}")
  uu.stdout_contains(r7, "0 empty-file")
  uu.stdout_contains(r7, "0 file-with-holes")
  uu.stdout_contains(r7, f"{size1k} zero-file")
  let r8 = ls_run(s, ["-s1h", "--si", "some-dir1"], vars: {})?
  uu.succeeds(r8)
  uu.stdout_contains(r8, "total 4.2M")
  uu.stdout_contains(r8, "0 empty-file")
  uu.stdout_contains(r8, "0 file-with-holes")
  uu.stdout_contains(r8, "4.2M zero-file")
  let r9 = ls_run(s, ["-s1", "--block-size=human-readable", "some-dir1"], vars: {})?
  uu.succeeds(r9)
  uu.stdout_contains(r9, f"total {size4m}")
  uu.stdout_contains(r9, "0 empty-file")
  uu.stdout_contains(r9, "0 file-with-holes")
  uu.stdout_contains(r9, f"{size4m} zero-file")
  let r10 = ls_run(s, ["-s1", "--block-size=si", "some-dir1"], vars: {})?
  uu.succeeds(r10)
  uu.stdout_contains(r10, "total 4.2M")
  uu.stdout_contains(r10, "0 empty-file")
  uu.stdout_contains(r10, "0 file-with-holes")
  uu.stdout_contains(r10, "4.2M zero-file")
}

# origin: uutils test_ls::test_ls_align_unquoted_multiline
test test_uu_ls_ls_align_unquoted_multiline { |ctx|
  let s = uu.scene(ctx)?
  for name in ["one", "two", "three_long", "four_long", "five", "s ix", "s even", "eight_long_long", "nine", "ten"] { uu.touch(s, name)? }
  let r = terminal_listing(s, ["--color"])?
  uu.succeeds(r)
  uu.stdout_only(r, " eight_long_long   four_long   one\t's ix'\t three_long\r\n five\t\t   nine       's even'\t ten\t two\r\n")
}
