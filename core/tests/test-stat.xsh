type TerminalRun = {status: Int, stdout: Str, stderr: Str}
type RawTerminalRun = {status: Int, stdout: Bytes, stderr: Str}

proc run_stat_on_terminal(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[TerminalRun] {
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let root = test.temp_dir(ctx, name: "stat-terminal")?
  let stdout = fp"{root}/stdout"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/stat.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", stdout, fp"{pty.name}")
  let status = process.run(plan)?
  let stderr = unix.read_fd(pty.master, 8192)?.utf8()?
  Ok({status: status.exit_code()?, stdout: stdout.read_text()?, stderr: stderr.replace("\r\n", with: "\n")})
}

proc run_stat_with_path(ctx: TestContext, options: List[Path], name: Path) [fs, process, error] -> Result[RawTerminalRun] {
  let root = test.temp_dir(ctx, name: "stat-path")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin, fp"{ctx.core_dir}/stat.xsh", p"--"].extend(options).extend([name])
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", stdout, stderr)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: stdout.read_bytes()?, stderr: stderr.read_text()?})
}

test test_stat { |ctx|
  let target = test.temp_file(ctx, name: "stat.txt", contents: b"hello")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- $target
  assert "kind file" in output
  assert "size 5" in output
  let formatted = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%s %F %n" $target
  assert "5 regular file" in formatted
  assert "stat.txt" in formatted
  let modes = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%a %A %U %G" $target
  assert "rw" in modes
}

test test_stat_dash_uses_standard_input { |ctx|
  let input = test.temp_file(ctx, name: "stat-stdin", contents: b"hello")?
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%F %s" - < $input

  assert result.status.exited_with(0), result.stderr
  assert result.stdout == "regular file 5\n"
}

pure stat_hex(value: Int) -> Str {
  let digits = "0123456789abcdef"
  var rest = value
  var output = ""

  loop {
    let index = rest % 16
    output = f"{digits[index..index + 1]}{output}"
    rest /= 16
    break when rest == 0
  }

  output
}

test test_stat_formats_complete_device_metadata { |ctx|
  let target = p"/dev/null"
  let meta = fs.stat(target)?
  let format = "%f %b %B %d %Hd %Ld %D %h %i %o %r %Hr %Lr %R %t %T"
  let expected = f"{stat_hex(meta.mode)} {meta.blocks_512} 512 {meta.dev} {fs.dev_major(meta.dev)} {fs.dev_minor(meta.dev)} {stat_hex(meta.dev)} {meta.nlink} {meta.ino} {meta.blksize} {meta.rdev} {fs.dev_major(meta.rdev)} {fs.dev_minor(meta.rdev)} {stat_hex(meta.rdev)} {stat_hex(fs.dev_major(meta.rdev))} {stat_hex(fs.dev_minor(meta.rdev))}"
  let output = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c $format $target

  assert output.status.exited_with(0), output.stderr
  assert output.stdout == f"{expected}\n"

  let printf = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- --printf $format $target
  assert printf.status.exited_with(0), printf.stderr
  assert printf.stdout == expected
}

test test_stat_quotes_names_and_preserves_symlink_target { |ctx|
  let root = test.temp_dir(ctx, name: "stat-quote")?
  let plain = fp"{root}/plain"
  plain.write("plain")
  let quoted = fp"{root}/it's"
  quoted.write("quoted")
  let link = fp"{root}/link"
  link.symlink(to: p"it's")

  let plain_name = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%N" $plain
  assert plain_name.status.exited_with(0), plain_name.stderr
  assert plain_name.stdout == f"{plain}\n"

  let quoted_name = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%N" $quoted
  assert quoted_name.status.exited_with(0), quoted_name.stderr
  assert quoted_name.stdout == f"\"{quoted}\"\n"

  let link_name = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%N" $link
  assert link_name.status.exited_with(0), link_name.stderr
  assert link_name.stdout == f"{link} -> \"it's\"\n"
}

test test_stat_mount_point_quoted_name_and_filesystem_format { |ctx|
  let root = test.temp_dir(ctx, name: "stat-format")?
  let target = fp"{root}/it's"
  target.write("data")
  let mount = fs.mount_for(target.resolve()?)?
  let mount_format = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%m" $target
  assert mount_format.status.exited_with(0), mount_format.stderr
  assert mount_format.stdout == f"{mount.mounted_on}\n"

  let quoted = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%Qn|%-6Qn" $target
  assert quoted.status.exited_with(0), quoted.stderr
  assert quoted.stdout == f"\"{target}\"|\"{target}\"\n"
  let literal = run.capture --text QUOTING_STYLE=literal ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%N" $target
  assert literal.status.exited_with(0), literal.stderr
  assert literal.stdout == f"{target}\n"

  let stats = fs.statvfs(target)?
  let filesystem = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -f -c "%b %c %i %l %n %s %S %t %T" $target
  assert filesystem.status.exited_with(0), filesystem.stderr
  assert filesystem.stdout == f"{stats.blocks} {stats.files} {stat_hex(stats.fsid)} {stats.name_max} {target} {stats.block_size} {stats.fragment_size} {stat_hex(stats.type_magic ?? 0)} {mount.fstype}\n"
}

test test_stat_terse_default_output_and_missing_operand_status { |ctx|
  let file = test.temp_file(ctx, name: "stat-terse", contents: b"data")?
  let terse = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -t $file
  assert terse.status.exited_with(0), terse.stderr
  assert terse.stdout.starts_with(f"{file} 4 ")

  let missing = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- no-such-stat-file
  assert missing.status.exited_with(1)
  assert missing.stderr == "stat: cannot statx 'no-such-stat-file': No such file or directory\n"
}

test test_stat_formats_timestamp_precision_and_padding { |ctx|
  let file = test.temp_file(ctx, name: "stat-time", contents: b"time")?
  fs.set_times(file, mtime_ns: 67413023456789)
  let format = "%Y %.Y %.0Y %.1Y %.3Y %.9Y %13.6Y %013.6Y %-13.6Y %18.10Y %I18.10Y %018.10Y %-18.10Y"
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c $format $file

  assert result.status.exited_with(0), result.stderr
  assert result.stdout == "67413 67413.023456789 67413 67413.0 67413.023 67413.023456789  67413.023456 067413.023456 67413.023456    67413.0234567890   67413.0234567890 0067413.0234567890 67413.0234567890  \n"

  fs.set_times(file, mtime_ns: -876543211)
  let before_epoch = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%.0Y %.1Y %.3Y %.9Y" $file
  assert before_epoch.status.exited_with(0), before_epoch.stderr
  assert before_epoch.stdout == "-1 -0.8 -0.876 -0.876543211\n"
}

test test_stat_printf_decodes_control_and_octal_escapes { |ctx|
  let file = test.temp_file(ctx, name: "stat-printf", contents: b"text")?
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- --printf=".%s\\n\\t\\a\\b\\f\\r\\012\\101\\000" $file

  assert result.status.exited_with(0), result.stderr
  assert result.stdout == ".4\n\t\x07\x08\x0c\r\nA\0"
}

test test_stat_format_errors_keep_the_directive_and_output_prefix { |ctx|
  let output = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "€%-" p"."
  assert output.status.exited_with(1), f"{output.stdout}|{output.stderr}"
  assert output.stdout == "€"
  assert output.stderr == "stat: '%-': invalid directive\n"

  let printf = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- --printf -%n p"/"
  assert printf.status.exited_with(0), printf.stderr
  assert printf.stdout == "-/"
}

test test_stat_format_errors_point_into_terminal_arguments { |ctx|
  let format = run_stat_on_terminal(ctx, ["-c", "%d%.3", "/dev/null"])?
  assert format.status == 1
  assert format.stdout != ""
  assert format.stderr == """stat: '%.3': invalid directive
   ╭─[ stat:1:11 ]
   │
 1 │ stat -c %d%.3 /dev/null
   │           ───
   │
   │ Help: a directive is %[FLAGS][WIDTH][.PRECISION]LETTER, as in %-10.2s; a literal % is written %%
───╯
""", format.stderr

  let printf = run_stat_on_terminal(ctx, ["--printf=%12", "/dev/null"])?
  assert printf.status == 1
  assert printf.stderr == """stat: '%12': invalid directive
   ╭─[ stat:1:15 ]
   │
 1 │ stat --printf=%12 /dev/null
   │               ───
   │
   │ Help: a directive is %[FLAGS][WIDTH][.PRECISION]LETTER, as in %-10.2s; a literal % is written %%
───╯
""", printf.stderr
}

test test_stat_reports_non_utf8_missing_names { |ctx|
  let names = [Path.parse_bytes(b"missing-\xff")?, Path.parse_bytes(b"missing-\xc3\xa9")?]
  let quoted_names = ["'missing-'$'\\377'", "'missing-'$'\\303\\251'"]
  for index in range(names.len()) {
    let name = names[index]
    for options in [[], [p"-L"], [p"-f"]] {
      let result = run_stat_with_path(ctx, options, name)?
      assert result.status == 1, f"{result.status}|{result.stderr}"
      let message = if options == [p"-f"] { "cannot read file system information for" } else { "cannot statx" }
      let expected = f"stat: {message} {quoted_names[index]}: No such file or directory\n"
      assert result.stderr == expected, result.stderr
    }
  }
}

test test_stat_preserves_non_utf8_names_in_output_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "stat-raw-name")?
  let name = Path.parse_bytes(bytes.concat([root.bytes(), b"/raw-\xff"]))?
  name.write("data")
  let quoted_name = bytes.from_text(f"'{root}/raw-'$'\\377'")

  let default = run_stat_with_path(ctx, [], name)?
  assert default.status == 0, default.stderr
  assert default.stdout.starts_with(bytes.concat([b"  File: ", quoted_name, b"\n"]))
  assert default.stdout.ends_with(bytes.concat([b"path ", name.bytes(), b"\n"]))

  let format = run_stat_with_path(ctx, [p"--format=%n"], name)?
  assert format.status == 0, format.stderr
  assert format.stdout == bytes.concat([name.bytes(), b"\n"])

  let printf = run_stat_with_path(ctx, [p"--printf=%N"], name)?
  assert printf.status == 0, printf.stderr
  assert printf.stdout == quoted_name
}

test test_stat_cached_accepts_gnu_modes_and_unique_prefixes { |ctx|
  let file = test.temp_file(ctx, name: "stat-cached", contents: b"cached")?
  let plain = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%s %n" $file
  assert plain.status.exited_with(0), plain.stderr

  for mode in ["default", "never", "always", "nev", "a", "d"] {
    let option = f"--cached={mode}"
    let result = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- $option -c "%s %n" $file
    assert result.status.exited_with(0), f"{mode}: {result.stderr}"
    assert result.stdout == plain.stdout, f"{mode}: {result.stdout}"
  }

  let separate = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- --cached never -c "%s %n" $file
  assert separate.status.exited_with(0), separate.stderr
  assert separate.stdout == plain.stdout
}

test test_stat_cached_rejects_invalid_modes_with_gnu_diagnostics { |ctx|
  let file = test.temp_file(ctx, name: "stat-cached-invalid", contents: b"invalid")?
  let choices = "Valid arguments are:\n  - 'default'\n  - 'never'\n  - 'always'\nTry 'stat --help' for more information.\n"

  for mode in ["bogus", "ALWAYS", "x=y"] {
    let option = f"--cached={mode}"
    let result = run.capture --text LC_ALL=C XSH_EXECUTION_PHRASE=stat ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- $option $file
    assert result.status.exited_with(1), result.stderr
    assert result.stdout == ""
    assert result.stderr == f"stat: invalid argument '{mode}' for '--cached'\n{choices}", result.stderr
  }

  let empty = run.capture --text LC_ALL=C XSH_EXECUTION_PHRASE=stat ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- --cached= $file
  assert empty.status.exited_with(1), empty.stderr
  assert empty.stdout == ""
  assert empty.stderr == f"stat: ambiguous argument '' for '--cached'\n{choices}", empty.stderr

  let missing = run.capture --text LC_ALL=C XSH_EXECUTION_PHRASE=stat ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- --cached
  assert missing.status.exited_with(1), missing.stderr
  assert missing.stderr == "stat: option '--cached' requires an argument\nTry 'stat --help' for more information.\n", missing.stderr
}

test test_stat_cached_is_checked_in_option_order { |ctx|
  let file = test.temp_file(ctx, name: "stat-cached-order", contents: b"order")?

  let later = run.capture --text LC_ALL=C XSH_EXECUTION_PHRASE=stat ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- --cached=never --cached=bogus $file
  assert later.status.exited_with(1), later.stderr
  assert later.stderr.starts_with("stat: invalid argument 'bogus' for '--cached'\n"), later.stderr

  let before_help = run.capture --text LC_ALL=C XSH_EXECUTION_PHRASE=stat ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- --cached=bogus --help
  assert before_help.status.exited_with(1), before_help.stderr
  assert before_help.stderr.starts_with("stat: invalid argument 'bogus' for '--cached'\n"), before_help.stderr

  let help = run.capture --text LC_ALL=C XSH_EXECUTION_PHRASE=stat ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- --help --cached=bogus
  assert help.status.exited_with(0), help.stderr
  assert help.stdout.starts_with("Usage: stat [OPTION]... FILE...\n"), help.stdout

  let no_operand = run.capture --text LC_ALL=C XSH_EXECUTION_PHRASE=stat ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- --cached=bogus
  assert no_operand.status.exited_with(1), no_operand.stderr
  assert no_operand.stderr.starts_with("stat: invalid argument 'bogus' for '--cached'\n"), no_operand.stderr
}

# GNU keeps going after an unrecognized escape: the character is printed and a
# warning is issued for each one, in format order.
test test_stat_printf_warns_for_each_unrecognized_escape { |ctx|
  let file = test.temp_file(ctx, name: "stat-escape-warning", contents: b"text")?
  let result = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- "--printf=\\e\\\"\\q\\x" $file
  assert result.status.exited_with(0), result.stderr
  assert bytes.from_text(result.stdout) == b"\x1b\"qx", result.stdout
  assert result.stderr == "stat: warning: unrecognized escape '\\q'\nstat: warning: unrecognized escape '\\x'\n", result.stderr
  let trailing = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- "--printf=\\" $file
  assert trailing.stdout == "\\", trailing.stdout
  assert trailing.stderr == "stat: warning: backslash at end of format\n", trailing.stderr
}

# QUOTING_STYLE is validated only when a directive quotes the name, and "%%"
# is a literal percent rather than the start of a directive.
test test_stat_quoting_style_is_checked_only_for_quoting_directives { |ctx|
  let file = test.temp_file(ctx, name: "stat-quoting-style", contents: b"text")?
  let literal = run.capture --text LC_ALL=C QUOTING_STYLE=bogus ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%%N" $file
  assert literal.status.exited_with(0), literal.stderr
  assert literal.stdout == "%N\n"
  assert literal.stderr == ""
  let quoted = run.capture --text LC_ALL=C QUOTING_STYLE=bogus ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -- -c "%N" $file
  assert quoted.status.exited_with(0), quoted.stderr
  assert quoted.stderr == f"stat: ignoring invalid value of environment variable QUOTING_STYLE: 'bogus'\n", quoted.stderr
}
