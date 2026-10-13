use support.uu
use lib.gnu as gnu

const WC_METADATA_SIZE = 2 * 1048576
const WC_OVERLAY_SIZE = 2305843009213693952

# Preserve wc's diagnostic prefix by running the overlay from an exact wc.xsh path.
proc wc_overlay_in(s: uu.Scene, source: Str, args: List[Str]) [fs, process, error] -> Result[uu.Ran] {
  let script = uu.at(s, "wc.xsh")
  script.write(source)
  let out = uu.at(s, "overlay-stdout")
  let err = uu.at(s, "overlay-stderr")
  let modules = f"{s.ctx.core_dir}:{s.ctx.core_dir.parent()}"
  let argv = [s.ctx.xsh_bin, script, p"--"].extend([fp"{arg}" for arg in args])
  let plan = process.command_argv(s.ctx.xsh_bin, argv, s.root,
    {LC_ALL: "C", TZ: "UTC", XSH_MODULE_PATH: modules}, b"", out, err)
  let status = process.run(plan)?
  Ok({util: "wc", args, status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# Keep byte-count edges covered when a filesystem rejects the giant sparse size.
proc wc_size_overlay(ctx: TestContext, file_size: Int, reported_size: Int) [fs, error] -> Result[Str] {
  let source = fp"{ctx.core_dir}/wc.xsh".read_text()?
  let header = "pure byte_count(size: Int) -> ByteCount {"
  let pieces = source.split(header)
  assert pieces.len() == 2, "wc byte_count header must be unique"
  let adapted = f"pure byte_count(file_size: Int) -> ByteCount {{\n  let size = if file_size == {file_size} {{ {reported_size} }} else {{ file_size }}"
  Ok(pieces[0] + adapted + pieces[1])
}

# origin: gnu wc/wc-cpu.log
test test_gnu_wc_wc_cpu_log { |ctx|
  let s = uu.scene(ctx)?
  let disabled = "glibc.cpu.hwcaps=-ASIMD,-AVX2,-AVX512F"
  let debug = uu.invoke(s, "wc", ["-l", "--debug", "/dev/null"], vars: {GLIBC_TUNABLES: disabled})?
  uu.succeeds(debug)
  assert !rx"using.*hardware support".matches(debug.stderr as Str)
  let shuffled = uu.invoke(s, "shuf", ["-i", "0-1000"])?
  uu.succeeds(shuffled)
  let selected = shuffled.stdout.utf8()?.lines()[0].parse_int() ?? -1
  assert selected >= 0 and selected <= 1000
  uu.write(s, "lines", [f"{n}\n" for n in range(1, selected + 1)].join(""))
  let outputs = collect {
    for tunables in ["", "glibc.cpu.hwcaps=-AVX512F", disabled] {
      let r = uu.invoke_from_path(s, "wc", ["-l"], uu.at(s, "lines"), vars: {GLIBC_TUNABLES: tunables})?
      uu.succeeds(r)
      uu.no_stderr(r)
      yield r.stdout
    }
  }
  for output in outputs { assert output == outputs[0] }
}

# origin: gnu wc/wc-files0-from.log
test test_gnu_wc_wc_files0_from_log { |ctx|
  let s = uu.scene(ctx)?
  let rows = [
    {args: ["--files0-from=-", "no-such", "f-extra-arg.1"], input: b"", redirected: false, output: "", error: "wc: extra operand 'no-such'\nfile operands cannot be combined with --files0-from\nTry 'wc --help' for more information.\n", status: 1},
    {args: ["--files0-from=missing"], input: b"", redirected: false, output: "", error: "wc: cannot open 'missing' for reading: No such file or directory\n", status: 1},
    {args: ["--files0-from=-"], input: b"missing\0missing\0", redirected: true, output: "0 0 0 total\n", error: "wc: missing: No such file or directory\nwc: missing: No such file or directory\n", status: 1},
    {args: ["--files0-from=-"], input: b"g\0g\0", redirected: true, output: "0 0 0 g\n0 0 0 g\n0 0 0 total\n", error: "", status: 0},
    {args: ["--files0-from=-"], input: b"-", redirected: true, output: "", error: "wc: when reading file names from standard input, no file name of '-' allowed\n", status: 1},
    {args: ["--files0-from=empty.1"], input: b"", redirected: false, output: "", error: "", status: 0},
    {args: ["--files0-from=/dev/null"], input: b"", redirected: false, output: "", error: "", status: 0},
    {args: ["--files0-from=-"], input: b"\0", redirected: true, output: "", error: "wc: -:1: invalid zero-length file name\n", status: 1},
    {args: ["--files0-from=-"], input: b"\0\0", redirected: true, output: "0 0 0 total\n", error: "wc: -:1: invalid zero-length file name\nwc: -:2: invalid zero-length file name\n", status: 1},
    {args: ["--files0-from=-"], input: b"g", redirected: true, output: "0 0 0 g\n", error: "", status: 0},
    {args: ["--files0-from=-"], input: b"g\0", redirected: true, output: "0 0 0 g\n", error: "", status: 0},
    {args: ["--files0-from=-"], input: b"g\0g", redirected: true, output: "0 0 0 g\n0 0 0 g\n0 0 0 total\n", error: "", status: 0},
    {args: ["--files0-from=-"], input: b"g\0g\0", redirected: true, output: "0 0 0 g\n0 0 0 g\n0 0 0 total\n", error: "", status: 0},
    {args: ["--files0-from=-"], input: b"\0g\0", redirected: true, output: "0 0 0 g\n0 0 0 total\n", error: "wc: -:1: invalid zero-length file name\n", status: 1},
  ]
  uu.write(s, "f-extra-arg.1", "a")
  uu.touch(s, "empty.1")
  uu.touch(s, "g")
  for row in rows {
    let r = if row.redirected {
      uu.write_bytes(s, "f", row.input)
      uu.invoke_from_path(s, "wc", row.args, uu.at(s, "f"))?
    } else { uu.invoke(s, "wc", row.args)? }
    uu.fails_with_code(r, row.status)
    uu.stdout_is(r, row.output)
    uu.stderr_is(r, row.error)
  }
}

# origin: gnu wc/wc-files0.log
test test_gnu_wc_wc_files0_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "2b", "2\n")
  uu.write(s, "2w", "2 words\n")
  uu.write_bytes(s, "names", b"2b\02w\0")
  let expected = " 1  1  2 2b\n 1  2  8 2w\n 2  3 10 total\n"
  let file = uu.invoke(s, "wc", ["--files0-from=names"])?
  uu.succeeds(file)
  uu.stdout_only(file, expected)
  let redirected = uu.invoke_from_path(s, "wc", ["--files0-from=-"], uu.at(s, "names"))?
  uu.succeeds(redirected)
  uu.stdout_only(redirected, expected)
  uu.touch(s, "1\n2")
  let newline = uu.invoke(s, "wc", ["--files0-from=-"], stdin: b"1\n2\0")?
  uu.succeeds(newline)
  uu.stdout_only(newline, "0 0 0 '1'$'\\n''2'\n")
  uu.touch(s, "wc.big")
  uu.truncate(s, "wc.big", 1073741824)
  uu.touch(s, "wc.small")
  let sparse = uu.invoke(s, "wc", ["-c", "--files0-from=-"], stdin: b"wc.big\0wc.small\0", timeout: 10s)?
  uu.succeeds(sparse)
  uu.stdout_only(sparse, "1073741824 wc.big\n0 wc.small\n1073741824 total\n")
}

# origin: gnu wc/wc-nbsp.log
test test_gnu_wc_wc_nbsp_log { |ctx|
  let s = uu.scene(ctx)?
  for case in [
    {locale: "en_US.iso8859-1", character: b"\xa0"},
    {locale: "en_US.UTF-8", character: bytes.from_text(" ")},
    {locale: "en_US.UTF-8", character: bytes.from_text(" ")},
    {locale: "en_US.UTF-8", character: bytes.from_text(" ")},
    {locale: "en_US.UTF-8", character: bytes.from_text("⁠")},
    {locale: "en_US.UTF-8", character: b" "},
    {locale: "en_US.UTF-8", character: bytes.from_text(" ")},
    {locale: "ru_RU.KOI8-R", character: b"\x9a"},
  ] {
    let input = bytes.concat([b"=", case.character, b"="])
    let width = uu.invoke(s, "wc", ["-L"], stdin: input, vars: {LC_ALL: case.locale})?
    uu.succeeds(width)
    if width.stdout.utf8()?.trim() == "3" {
      let words = uu.invoke(s, "wc", ["-w"], stdin: input, vars: {LC_ALL: case.locale})?
      uu.succeeds(words)
      let octets = [f"{case.character.byte_at(at) ?? 0}" for at in range(case.character.len())].join(",")
      assert words.stdout.utf8()?.trim() == "2", f"LC_ALL={case.locale}, character bytes [{octets}], display width {width.stdout.utf8()?.trim()}"
    }
  }
}

# origin: gnu wc/wc-parallel.log
test test_gnu_wc_wc_parallel_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tmp")
  let names = [f"tmp/{n}" for n in range(1, 2001)]
  for name in names { uu.touch(s, name) }
  let wc_words = uu.argv(s, "wc", [])?
  let xargs_words = uu.argv(s, "xargs", [p"-n2000", p"-P2", @wc_words])?
  let cat_words = uu.argv(s, "cat", [])?
  let reader = [shell_word(word.display()) for word in cat_words].join(" ")
  let script = Path("\"$@\" | " + reader)
  let input = bytes.from_text([@names, @names, @names].join("\n") + "\n")
  let command = process.command_argv(p"/bin/sh", [p"sh", p"-c", script, p"wc-pipe", @xargs_words],
    s.root, {}, input, uu.at(s, "out"), uu.at(s, "err"), timeout: 30s)
  assert process.run(command)?.exit_code()? == 0
  assert uu.read(s, "err")? == b""
  let lines = uu.read_text(s, "out")?.lines()
  assert lines.len() == 6003
  for line in lines { assert "0 0 0 " in line }
}

pure shell_word(value: Str) -> Str {
  "'" + value.replace("'", with: "'\\''") + "'"
}

# origin: gnu wc/wc-proc.log
test test_gnu_wc_wc_proc_log { |ctx|
  let s = uu.scene(ctx)?
  for input in [p"/proc/version", p"/sys/kernel/profiling"] {
    if fs.access(input, read: true) {
      uu.write_bytes(s, "copy", input.read_bytes()?)
      let regular = uu.invoke_from_path(s, "wc", ["-c"], uu.at(s, "copy"))?
      let virtual = uu.invoke_from_path(s, "wc", ["-c"], input)?
      uu.succeeds(regular)
      uu.succeeds(virtual)
      assert regular.stdout == virtual.stdout
    }
  }
  uu.touch(s, "no_read")
  uu.truncate(s, "no_read", 2)
  uu.touch(s, "do_read")
  uu.truncate(s, "do_read", 1048576)
  let sizes = uu.invoke(s, "wc", ["-c", "no_read", "do_read"])?
  uu.succeeds(sizes)
  uu.stdout_only(sizes, "      2 no_read\n1048576 do_read\n1048578 total\n")
  let words = uu.argv(s, "wc", [p"-c"])?
  for case in [{name: "no_read", expected: "2\n0\n"}, {name: "do_read", expected: "1048576\n0\n"}] {
    # Both children inherit the same open file description and its current offset.
    let script = Path(r""""$@"; "$@"; """)
    let command = process.command_argv(p"/bin/sh", [p"sh", p"-c", script, p"wc-offset", @words],
      s.root, {}, uu.at(s, case.name), uu.at(s, "out"), uu.at(s, "err"), timeout: 10s)
    assert process.run(command)?.exit_code()? == 0
    uu.file_is(s, "out", case.expected)
    uu.file_is(s, "err", "")
  }
  uu.truncate(s, "do_read", 1099511627776)
  let terabyte = uu.invoke(s, "wc", ["-c", "do_read"], timeout: 10s)?
  uu.succeeds(terabyte)
}

# origin: gnu wc/wc-total.log
test test_gnu_wc_wc_total_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "2b", "2\n")
  uu.write(s, "2w", "2 words\n")
  uu.fails_with_code(uu.invoke(s, "wc", ["--total", "2b", "2w"])?, 1)
  let never = uu.invoke(s, "wc", ["--total=never", "2b", "2w"])?
  uu.succeeds(never)
  uu.stdout_only(never, " 1  1  2 2b\n 1  2  8 2w\n")
  let only = uu.invoke(s, "wc", ["--total=only", "2b", "2w"])?
  uu.succeeds(only)
  uu.stdout_only(only, "2 3 10\n")
  let always = uu.invoke(s, "wc", ["--total=always", "2b"])?
  uu.succeeds(always)
  assert always.stdout.utf8()?.lines().len() == 2
  uu.touch(s, "big")
  let oversized = uu.at(s, "big").truncate(WC_OVERLAY_SIZE)
  match oversized {
    Ok(_) => {
      let files = ["big" for n in range(8)]
      let overflow = uu.invoke(s, "wc", ["--total=only", "-c", @files], timeout: 10s)?
      let hidden = uu.invoke(s, "wc", ["--total=never", "-c", @files], timeout: 10s)?
      assert overflow.status == 1, f"total=only status {overflow.status}, stderr {overflow.stderr as Str}; total=never status {hidden.status}, stderr {hidden.stderr as Str}"
      uu.stdout_is(overflow, "18446744073709551615\n")
      uu.stderr_is(overflow, "wc: total bytes: Value too large for defined data type\n")
      uu.succeeds(hidden)
      uu.no_stderr(hidden)
      uu.stdout_is(hidden, [" 2305843009213693952 big\n" for n in range(8)].join(""))
    }
    Err(failure) => {
      if gnu.errno(failure) != 27 { return Err(failure) }
      uu.at(s, "big").truncate(WC_METADATA_SIZE)?
      let source = wc_size_overlay(ctx, WC_METADATA_SIZE, WC_OVERLAY_SIZE)?
      let title = uu.at(s, "big").display()
      let file_args = [title for n in range(8)]
      let overflow = wc_overlay_in(s, source, ["--total=only", "-c"].extend(file_args))?
      let hidden = wc_overlay_in(s, source, ["--total=never", "-c"].extend(file_args))?
      assert overflow.status == 1, f"total=only status {overflow.status}, stderr {overflow.stderr.utf8()?}"
      assert overflow.stdout.utf8()? == "18446744073709551615\n", overflow.stdout.utf8()?
      assert overflow.stderr.utf8()? == "wc: total bytes: Value too large for defined data type\n", overflow.stderr.utf8()?
      assert hidden.status == 0, f"total=never status {hidden.status}, stderr {hidden.stderr.utf8()?}"
      let item = f" 2305843009213693952 {title}\n"
      assert hidden.stdout.utf8()? == [item for n in range(8)].join(""), hidden.stdout.utf8()?
      assert hidden.stderr == b""
    }
  }
}
