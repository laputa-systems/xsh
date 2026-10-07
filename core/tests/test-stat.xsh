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
