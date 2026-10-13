use support.uu

proc pieces(s: uu.Scene) [fs, error] -> Result[List[Path], Error] {
  Ok(collect { for entry in fs.children(s.root, ordered: true)? { yield entry.path when entry.name.starts_with("x") } })
}

proc clear_pieces(s: uu.Scene) [fs, error] {
  for file in pieces(s)? { file.remove() }
  Ok()
}

proc empty_pieces(s: uu.Scene, count: Int) [fs, error] {
  let found = pieces(s)?
  assert found.len() == count
  for file in found { assert file.read_bytes()? == b"" }
}

proc assert_parts(s: uu.Scene, expected: List[Bytes]) [fs, error] {
  let names = "abcdefghijklmnopqrstuvwxyz"
  for i in range(expected.len()) { assert uu.read(s, f"xa{names.byte_slice(i, 1)}")? == expected[i] }
  assert ! uu.exists(s, f"xa{names.byte_slice(expected.len(), 1)}")?
}

# origin: gnu split/additional-suffix.log
test test_gnu_split_additional_suffix_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "1\n2\n3\n4\n5\n")
  uu.succeeds(uu.invoke(s, "split", ["--lines=2", "--additional-suffix=.txt", "in"])?)
  for row in [{name: "xaa.txt", data: b"1\n2\n"}, {name: "xab.txt", data: b"3\n4\n"}, {name: "xac.txt", data: b"5\n"}] { assert uu.read(s, row.name)? == row.data }
  for suffix in ["a/b", "a/"] {
    let r = uu.invoke(s, "split", ["--lines=2", f"--additional-suffix={suffix}", "in"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "contains directory separator")
  }
}

# origin: gnu split/lines.log
test test_gnu_split_lines_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "1\n2\n3\n4\n5\n")
  uu.succeeds(uu.invoke(s, "split", ["--lines=2", "in"])?)
  assert_parts(s, [b"1\n2\n", b"3\n4\n", b"5\n"])
}

# origin: gnu split/non-utf8.log
test test_gnu_split_non_utf8_log { |ctx|
  let s = uu.scene(ctx)?
  let raw = b"\xff|\xed\xba\xad|\xc2\x89|\xed\xa6\xbf\xed\xbf\xbf"
  uu.at_bytes(s, raw)?.write(b"a\n")
  uu.succeeds(uu.invoke_paths(s, "split", [p"-b1", p"-", raw as Path], stdin: b"AB")?)
  for suffix in [b"aa", b"ab"] { assert uu.at_bytes(s, bytes.concat([raw, suffix]))?.is_file()? }
  let option = bytes.concat([b"--additional-suffix=", raw]) as Path
  uu.succeeds(uu.invoke_paths(s, "split", [p"-b1", option, p"-", p"q"], stdin: b"AB")?)
  for prefix in [b"qaa", b"qab"] { assert uu.at_bytes(s, bytes.concat([prefix, raw]))?.is_file()? }
}

# origin: gnu split/numeric.log
test test_gnu_split_numeric_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "1\n2\n3\n4\n5\n")
  let expected = [b"1\n2\n", b"3\n4\n", b"5\n"]
  for mode in ["numeric", "hex"] {
    for start in [0, 9] {
      let flag = f"--{mode}-suffixes" + (if start == 0 { "" } else { f"={start}" })
      uu.succeeds(uu.invoke(s, "split", [flag, "--lines=2", "in"])?)
      for i in range(3) {
        let value = start + i
        let digit = if mode == "hex" { "0123456789abcdef".byte_slice(value, 1) } else { f"{value}" }
        let suffix = if value < 10 or mode == "hex" { "0" + digit } else { digit }
        assert uu.read(s, f"x{suffix}")? == expected[i]
      }
    }
    uu.fails_with_code(uu.invoke(s, "split", ["-a", "3", f"--{mode}-suffixes=1000", "in"])?, 1)
    for value in ["-1", "one"] { uu.fails_with_code(uu.invoke(s, "split", [f"--{mode}-suffixes={value}", "in"])?, 1) }
  }
}

# origin: gnu split/guard-input.log
test test_gnu_split_guard_input_log { |ctx|
  let s = uu.scene(ctx)?
  let input = [f"{i}\n" for i in range(1, 11)].join("")
  uu.write(s, "xaa", input)
  uu.symlink(s, "xaa", "in2")
  uu.hard_link(s, "xaa", "in3")
  for name in ["xaa", "in2", "in3"] { uu.fails_with_code(uu.invoke(s, "split", ["-C", "6", name])?, 1) }
  uu.fails_with_code(uu.invoke_from_path(s, "split", ["-C", "6", "-"], stdin: uu.at(s, "xaa"))?, 1)
  assert uu.read(s, "xaa")? == bytes.from_text(input)
}

# origin: gnu split/split-io-err.log
test test_gnu_split_split_io_err_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "/dev/full", "xaa")
  let full = uu.invoke(s, "split", ["-b", "1"], stdin: b"1\n2\n")?
  uu.fails_with_code(full, 1)
  assert uu.exists(s, "xaa")?
  assert ! uu.exists(s, "xab")?
  uu.stderr_is(full, "split: xaa: No space left on device\n")
  uu.remove(s, "xaa")
  uu.mkdir(s, "xaa")
  uu.fails_with_code(uu.invoke(s, "split", ["-b", "1", "2"], stdin: b"1\n2\n")?, 1)
  assert uu.dir_exists(s, "xaa")?
  uu.succeeds(uu.invoke(s, "split", [])?)
}

# origin: gnu split/record-sep.log
test test_gnu_split_record_sep_log { |ctx|
  let s = uu.scene(ctx)?
  for separator in ["\n", "\\0", ":"] {
    let byte = if separator == "\\0" { b"\0" } else { bytes.from_text(separator) }
    for mode in ["--lines=2", "--line-bytes=4", "--number=l/3", "--number=r/3"] {
      uu.write(s, "in", "1\n2\n3\n4\n5\n")
      uu.succeeds(uu.invoke(s, "split", [mode, "in"])?)
      let expected = [bytes.concat([if chunk == b"\n" { byte } else { chunk } for chunk in uu.read(s, name)?.chunks(1)]) for name in ["xaa", "xab", "xac"]]
      clear_pieces(s)
      let input = bytes.concat([bytes.concat([bytes.from_text(f"{i}"), byte]) for i in range(1, 6)])
      uu.write_bytes(s, "in", input)
      uu.succeeds(uu.invoke(s, "split", [mode, "-t", separator, "in"])?)
      assert_parts(s, expected)
    }
  }
  for args in [["-t"], ["-txx"], ["-ta", "-tb"], ["-t\n", "-tb"]] { uu.fails_with_code(uu.invoke(s, "split", args)?, 1) }
  uu.succeeds(uu.invoke(s, "split", ["-t:", "-t:"])?)
}

# origin: gnu split/b-chunk.log
test test_gnu_split_b_chunk_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "split", ["-n", "10", "/dev/null"])?)
  empty_pieces(s, 10)
  clear_pieces(s)
  uu.write(s, "abc", "abc")
  for row in [{count: "4", chunks: [b"a", b"b", b"c", b""]}, {count: "2", chunks: [b"ab", b"c"]}] {
    uu.succeeds(uu.invoke(s, "split", ["-n", row.count, "abc"])?)
    assert_parts(s, row.chunks)
    clear_pieces(s)
  }
  uu.succeeds(uu.invoke(s, "split", ["-n", "2/3", "/dev/null"])?)
  assert pieces(s)?.is_empty()
  uu.succeeds(uu.invoke(s, "split", ["-e", "-n", "10", "/dev/null"])?)
  assert pieces(s)?.is_empty()
  uu.write(s, "input", "1\n2\n3\n4\n5\n")
  for file in ["input", "/proc/version", "/sys/kernel/profiling"] {
    let pathname = if file.starts_with("/") { fp"{file}" } else { uu.at(s, file) }
    continue unless pathname.exists()
    continue when fs.stat(pathname, follow_symlinks: true)?.kind != "file"
    for block in [1, 2, 4096] {
      continue when file != "input" and block != 4096
      uu.succeeds(uu.invoke(s, "split", ["-n", "3", f"---io-blksize={block}", file])?)
      if file == "input" { assert_parts(s, [b"1\n2\n", b"3\n4", b"\n5\n"]) }
      var all = b""
      for i in range(3) {
        let name = ["xaa", "xab", "xac"][i]
        let extracted = uu.invoke(s, "split", ["-n", f"{i + 1}/3", f"---io-blksize={block}", file])?
        uu.succeeds(extracted)
        assert extracted.stdout == uu.read(s, name)?
        all = bytes.concat([all, uu.read(s, name)?])
      }
      assert all == pathname.read_bytes()?
      assert ! uu.exists(s, "xad")?
    }
  }
}

# origin: gnu split/l-chunk.log
test test_gnu_split_l_chunk_log { |ctx|
  let s = uu.scene(ctx)?
  let invalid = uu.invoke(s, "split", ["-n", "l/1o"])?
  uu.fails_with_code(invalid, 1)
  uu.stderr_is(invalid, "split: invalid number of chunks: '1o'\n")
  uu.succeeds(uu.invoke(s, "split", ["-n", "l/1"])?)
  assert_parts(s, [b""])
  uu.succeeds(uu.invoke(s, "split", ["-n", "l/10", "/dev/null"])?)
  empty_pieces(s, 10)
  clear_pieces(s)
  uu.succeeds(uu.invoke(s, "split", ["-e", "-n", "l/10", "/dev/null"])?)
  assert pieces(s)?.is_empty()
  let text = "12345~1~12345~1~12345~1~12345~1~12345~~~12345~1~12345~1~12345~1~12345~1~12345~1~".replace("~", with: "\n")
  uu.write(s, "in", text)
  let wrong = uu.invoke(s, "split", ["-n", "l/16/15", "in"])?
  uu.fails_with_code(wrong, 1)
  assert rx"': [^\n]*".replace(wrong.stderr as Str, with: "'") == "split: invalid chunk number: '16'\n"
  let matrix = [
    {count: 6, sizes: [14, 16, 16, 8, 16, 10]},
    {count: 8, sizes: [14, 8, 8, 10, 14, 8, 8, 10]},
    {count: 12, sizes: [8, 6, 8, 8, 8, 8, 8, 2, 6, 8, 8, 2]},
    {count: 15, sizes: [6, 8, 8, 2, 6, 8, 2, 6, 8, 2, 6, 8, 0, 8, 2]},
    {count: 22, sizes: [6, 2, 6, 2, 6, 2, 6, 2, 6, 2, 6, 2, 6, 2, 6, 0, 8, 0, 2, 6, 0, 2]}
  ]
  for elide in [false, true] {
    for block in [1, 2, 5, 10, 80, 100] {
      for row in matrix {
        clear_pieces(s)
        let selected: Bytes? = if elide { null } else { uu.invoke(s, "split", [f"---io-blksize={block}", "-n", f"l/2/{row.count}", "in"])?.stdout }
        if ! elide { assert pieces(s)?.is_empty() }
        let flags = [f"---io-blksize={block}", @if elide { ["-e"] } else { [] }, "-n", f"l/{row.count}", "in"]
        let _ = uu.invoke(s, "split", flags)?
        let found = pieces(s)?
        let sizes = [fs.stat(file)?.size for file in found]
        assert sizes == [size for size in row.sizes if ! elide or size != 0], f"block {block}, count {row.count}, elide {elide}"
        if let chunk = selected { assert chunk == uu.read(s, "xab")? }
      }
    }
  }
  for row in [{part: 13, output: b""}, {part: 14, output: b"1\n12345\n"}, {part: 15, output: b"1\n"}] {
    let r = uu.invoke(s, "split", ["-n", f"l/{row.part}/15", "in"])?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, row.output)
  }
  uu.write(s, "in", "12\n34\n5")
  let last = uu.invoke(s, "split", ["-n", "l/7/7", "in"])?
  uu.stdout_is_bytes(last, b"5")
}

# origin: gnu split/r-chunk.log
test test_gnu_split_r_chunk_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "split", ["-n", "r/10", "/dev/null"])?)
  empty_pieces(s, 10)
  clear_pieces(s)
  uu.succeeds(uu.invoke(s, "split", ["-e", "-n", "r/10", "/dev/null"])?)
  assert pieces(s)?.is_empty()
  uu.write(s, "in", "1\n2\n3\n4\n5\n")
  let split = uu.invoke(s, "split", ["-n", "r/3", "in"])?
  uu.succeeds(split)
  uu.no_stdout(split)
  let expected = [b"1\n4\n", b"2\n5\n", b"3\n"]
  assert_parts(s, expected)
  for i in range(3) {
    let extracted = uu.invoke(s, "split", ["-n", f"r/{i + 1}/3", "in"])?
    uu.succeeds(extracted)
    uu.stdout_is_bytes(extracted, expected[i])
  }
  let last = uu.invoke(s, "split", ["-n", "r/2/3"], stdin: b"1\n2\n3\n4\n5")?
  uu.stdout_is_bytes(last, b"2\n5")
  clear_pieces(s)
  let argv = uu.argv(s, "split", [p"-n", p"r/30"])?
  let shell = process.which("sh")?
  let plan = process.command_argv(s.ctx.xsh_bin, [@argv[..3], shell, p"-c", p"ulimit -n 20; exec \"$@\"", p"rr-limit", @argv[3..]], s.root, {}, bytes.concat([b"y\n" for _ in range(90)]), uu.at(s, "out"), uu.at(s, "err"), timeout: 10s)
  assert process.run(plan)?.exit_code()? == 0
  let found = pieces(s)?
  assert found.len() == 30
  for file in found { assert fs.stat(file)?.size == 6 }
}

# The isolation launcher runs before the budget is set; the limit applies to
# the applet itself. A marker distinguishes failed setup from a started applet.
proc vm_status(s: uu.Scene, args: List[Str], kilobytes: Int, marker: Str? = null, discovery: Bool = false) [fs, process, env, time, error] -> Result[Int, Error] {
  let shell = process.which("sh")?
  let argv = uu.argv(s, "split", [fp"{word}" for word in args])?
  let wrapper = r"""trap '' SEGV; ulimit -c 0; ulimit -v "$1" || exit 77; shift; if test -n "$1"; then : > "$1" || exit; fi; shift; exec "$@"; """
  let words = [@argv[..3], shell, p"-c", fp"{wrapper}", p"vm-limit", Path(f"{kilobytes}"), fp"{marker ?? ""}", @argv[3..]]
  let plan = process.command_argv(s.ctx.xsh_bin, words, s.root, {}, b"", uu.at(s, "vm-out"), uu.at(s, "vm-err"), timeout: 11s)
  let began = time.now() / 1000
  let result = process.run(plan)
  return Ok(77) when discovery and time.now() / 1000 - began >= 10
  let status = result?
  Ok(status.shell_code()?)
}

# Interpreter startup can exceed a standalone utility's memory footprint.
# Extend the startup search before testing the same small allocation margin;
# unsupported limits and slow probes retain their separate unavailable result.
proc minimum_vm(s: uu.Scene, args: List[Str]) [fs, process, env, time, error] -> Result[Int?, Error] {
  let conf = process.which("getconf")?
  let plan = process.command_argv(conf, [conf, p"PAGESIZE"], s.root, {}, b"", uu.at(s, "page-out"), uu.at(s, "page-err"))
  assert process.run(plan)?.exit_code()? == 0
  let page_kib = uu.read_text(s, "page-out")?.trim() as Int / 1024
  let candidates = [multiplier * 5000 for multiplier in range(1, 11)].extend([100000, 200000, 400000, 800000])
  for candidate in candidates {
    let code = vm_status(s, args, candidate, discovery: true)?
    return Ok(null) when code == 77
    continue when code != 0
    var previous = candidate
    for step in range(1, candidate / 1000) {
      let limit = candidate - step * 1000
      let reduced = vm_status(s, args, limit, discovery: true)?
      return Ok(null) when reduced == 77
      return Ok(previous + page_kib) when reduced != 0
      previous = limit
    }
  }
  assert false, f"split cannot start within 800000 KiB; no startup baseline was established; stderr {uu.read_text(s, "vm-err")?}"
  Ok(null)
}

# origin: gnu split/line-bytes.log
test test_gnu_split_line_bytes_log { |ctx|
  let s = uu.scene(ctx)?
  if let vm = minimum_vm(s, ["-C", "K", "/dev/null"])? { assert vm_status(s, ["-C", "G", "/dev/null"], vm)? == 0 }
  uu.write(s, "in", "1\n2222\n3\n4")
  uu.write(s, "no_eol_in", "0123456789")
  let expected = [[1, 1, 1, 1, 1, 1, 1, 1, 1, 1], [2, 2, 2, 1, 2, 1], [2, 3, 2, 2, 1], [2, 4, 3, 1], [2, 5, 3], [2, 5, 3], [7, 3], [7, 3], [9, 1], [9, 1], [10]]
  let no_eol = [[1, 1, 1, 1, 1, 1, 1, 1, 1, 1], [2, 2, 2, 2, 2], [3, 3, 3, 1], [4, 4, 2], [5, 5], [6, 4], [7, 3], [8, 2], [9, 1], [10], [10]]
  for block in range(1, 11) {
    for size in range(1, 12) {
      for row in [{name: "in", sizes: expected[size - 1]}, {name: "no_eol_in", sizes: no_eol[size - 1]}] {
        clear_pieces(s)
        uu.succeeds(uu.invoke(s, "split", [f"---io={block}", f"-C{size}", row.name])?)
        let found = pieces(s)?
        assert [fs.stat(file)?.size for file in found] == row.sizes, f"block {block}, size {size}, file {row.name}"
        assert bytes.concat([file.read_bytes()? for file in found]) == uu.read(s, row.name)?
      }
    }
  }
  let first = bytes.concat([bytes.from_ints([32 for _ in range(131070)])?, b"\n"])
  let second = b"x\n"
  let third = bytes.concat([bytes.from_ints([32 for _ in range(131071)])?, b"\n"])
  uu.write_bytes(s, "bigin", bytes.concat([first, second, third]))
  uu.succeeds(uu.invoke(s, "split", ["-C", "131072", "---io=131072", "bigin"])?)
  for row in [{name: "xaa", data: first}, {name: "xab", data: second}, {name: "xac", data: third}] { assert uu.read(s, row.name)? == row.data }
}

# origin: gnu split/suffix-length.log
test test_gnu_split_suffix_length_log { |ctx|
  let s = uu.scene(ctx)?
  let alphabet = "abcdefghijklmnopqrstuvwxyz"
  uu.write(s, "in", alphabet + "0")
  let short = uu.invoke(s, "split", ["-b", "1", "-a", "1", "in"])?
  uu.fails(short)
  for name in ["xa", "xz"] { assert uu.file_exists(s, name)? }
  for name in ["xaa", "xaz"] { assert ! uu.exists(s, name)? }
  uu.stderr_is(short, "split: output file suffixes exhausted\n")
  clear_pieces(s)
  let files = [f"xa{alphabet.byte_slice(i, 1)}" for i in range(26)].extend(["xba"])
  let verbose = uu.invoke(s, "split", ["--verbose", "-b", "1", "-a", "2", "in"])?
  uu.succeeds(verbose)
  uu.stdout_is(verbose, [f"creating file '{file}'\n" for file in files].join(""))
  assert ! uu.exists(s, "xbb")?
  for i in range(files.len()) { assert uu.read(s, files[i])? == bytes.from_text((alphabet + "0").byte_slice(i, 1)) }
  for mode in ["-b1000", "-l1000", "-C1000"] { uu.succeeds(uu.invoke(s, "split", ["-a2", mode])?) }
  clear_pieces(s)
  uu.fails_with_code(uu.invoke(s, "split", ["-a2", "-n1000"])?, 1)
  assert ! uu.exists(s, "xaa")?
  if let vm = minimum_vm(s, ["-a", "1", "/dev/null"])? {
    let limit = vm + 6000
    let status = vm_status(s, ["-a", "66542562175252", "in"], limit, marker: "ulimit-worked")?
    assert uu.file_exists(s, "ulimit-worked")?, f"limit setup did not reach split at {limit} KiB; stderr {uu.read_text(s, "vm-err")?}"
    assert status == 1, f"minimum {vm} KiB; limit {limit} KiB; status {status}; stderr {uu.read_text(s, "vm-err")?}"
    assert ! uu.exists(s, "xaa")?
  }
}

# origin: gnu split/suffix-auto-length.log
test test_gnu_split_suffix_auto_length_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file.in")
  uu.truncate(s, "file.in", 12)
  uu.fails_with_code(uu.invoke(s, "split", ["file.in", "-b1", "--numeric=89"])?, 1)
  assert pieces(s)?.len() == 11
  clear_pieces(s)
  uu.truncate(s, "file.in", 91)
  for prefix in ["x", "xx", ""] {
    for suffix in [".txt", ""] {
      uu.succeeds(uu.invoke(s, "split", ["file.in", prefix, "-b1", "--numeric", f"--additional-suffix={suffix}"])?)
      let found = collect { for entry in fs.children(s.root)? { yield entry.path when entry.name.starts_with(prefix) and rx"[0-9]".matches(entry.name) and entry.name.ends_with(suffix) } }
      assert found.len() == 91
      for part in ["89", "9000"] { assert uu.exists(s, f"{prefix}{part}{suffix}")? }
      for file in found { file.remove() }
    }
  }
  uu.truncate(s, "file.in", 100)
  uu.succeeds(uu.invoke(s, "split", ["--numeric-suffixes=1", "--number=r/100", "file.in"])?)
  clear_pieces(s)
  uu.fails_with_code(uu.invoke(s, "split", ["--numeric-suffixes=100", "--number=r/100", "file.in"])?, 1)
  uu.truncate(s, "file.in", 0)
  uu.succeeds(uu.invoke(s, "split", ["--numeric-suffixes", "--number=110", "file.in"])?)
}

# origin: gnu split/fail.log
test test_gnu_split_fail_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "in")
  uu.succeeds(uu.invoke(s, "split", ["-a", "0", "in"])?)
  for args in [["-b", "0", "in"], ["-C", "0", "in"], ["-l", "0", "in"], ["-n", "0", "in"], ["-n", "1/0", "in"], ["-n", "0/1", "in"], ["-n", "2/1", "in"]] { uu.fails_with_code(uu.invoke(s, "split", args)?, 1) }
  clear_pieces(s)
  uu.succeeds(uu.invoke(s, "split", ["-C", "1"], stdin: b"x\n")?)
  for name in ["xaa", "xab"] { assert uu.file_exists(s, name)? }
  assert ! uu.exists(s, "xac")?
  uu.succeeds(uu.invoke(s, "split", ["-1", "in"])?)
  uu.fails_with_code(uu.invoke(s, "split", ["-0", "in"])?, 1)
  for option in ["--lines=18446744073709551615", "--bytes=9223372036854775807", "--line-bytes=9223372036854775808", "--line-bytes=18446744073709551616"] { uu.succeeds(uu.invoke(s, "split", [option, "in"])?) }
  let large = uu.invoke(s, "truncate", ["-s18446744073709551616", "large"])?
  if large.status == 0 { uu.succeeds(uu.invoke(s, "split", ["--number=18446744073709551616/18446744073709551616", "large"])?) }
  uu.succeeds(uu.invoke(s, "split", ["--number=r/9223372036854775807/18446744073709551615"])?)
  let huge_round_robin = uu.invoke(s, "split", ["--number=r/18446744073709551616"])?
  uu.fails_with_code(huge_round_robin, 1)
  uu.no_stdout(huge_round_robin)
  uu.stderr_is(huge_round_robin, "split: memory exhausted\n")
  uu.succeeds(uu.invoke(s, "split", ["-99999999999999999991", "in"])?)
}
