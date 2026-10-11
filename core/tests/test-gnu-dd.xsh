use support.uu as uu

proc shell(s: uu.Scene, code: Str, words: List[Path], stdin: Path? = null, stderr: Path? = null) [fs, process, env, error] -> uu.Ran {
  let out = uu.at(s, "wrapped-out")
  let err = stderr ?? uu.at(s, "wrapped-err")
  let argv = [p"/bin/sh", p"-c", Path(code), p"dd-wrapper"].extend(words)
  let plan = if let input = stdin {
    process.command_argv(p"/bin/sh", argv, s.root, {LC_ALL: "C"}, input, out, err, timeout: 20s)
  } else {
    process.command_argv(p"/bin/sh", argv, s.root, {LC_ALL: "C"}, b"", out, err, timeout: 20s)
  }
  let status = process.run(plan)?.exit_code()?
  {util: "dd", args: [], status: status, stdout: out.read_bytes()?, stderr: if stderr == null { err.read_bytes()? } else { b"" }}
}

# Two applets share a single inherited input descriptor, so the second begins
# after the first applet's reads and seeks rather than reopening the file.
proc sequential(s: uu.Scene, first: List[Str], second: List[Str], input: Path, quiet_first: Bool = false) [fs, process, env, error] -> uu.Ran {
  let left = uu.argv(s, "dd", [Path(word) for word in first])?
  let right = uu.argv(s, "dd", [Path(word) for word in second])?
  let one = ["\"" + r"$" + "{" + f"{index + 1}" + "}\"" for index in range(left.len())].join(" ")
  let two = ["\"" + r"$" + "{" + f"{index + left.len() + 1}" + "}\"" for index in range(right.len())].join(" ")
  shell(s, f"{one}{if quiet_first { " 2>/dev/null" } else { "" }} && {two}", left.extend(right), stdin: input)
}

# origin: gnu dd/conv-case.log
test test_gnu_dd_conv_case_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input-lower", "abcdefghijklmnopqrstuvwxyz\n")?
  uu.write(s, "input-upper", "ABCDEFGHIJKLMNOPQRSTUVWXYZ\n")?
  for row in [
    {source: "lower", target: "lower", conv: "lcase"},
    {source: "upper", target: "upper", conv: "ucase"},
    {source: "upper", target: "lower", conv: "lcase"},
    {source: "lower", target: "upper", conv: "ucase"},
  ] {
    let r = uu.invoke(s, "dd", [f"if=input-{row.source}", f"of=output-{row.target}", f"conv={row.conv}"])?
    uu.succeeds(r)
    assert uu.read(s, f"output-{row.target}")? == uu.read(s, f"input-{row.target}")?
  }
  let locale = run.capture --text LC_ALL=en_US.iso8859-1 /bin/sh -c "locale charmap 2>/dev/null"
  if locale.status.exited_with(0) and locale.stdout.trim().replace("iso", with: "ISO-") == "ISO-8859-1" {
    uu.write_bytes(s, "input-lower", b"\xe9\n")?
    uu.write_bytes(s, "input-upper", b"\xc9\n")?
    for row in [
      {source: "lower", target: "lower", conv: "lcase"},
      {source: "upper", target: "upper", conv: "ucase"},
      {source: "upper", target: "lower", conv: "lcase"},
      {source: "lower", target: "upper", conv: "ucase"},
    ] {
      let r = uu.invoke(s, "dd", [f"if=input-{row.source}", f"of=output-{row.target}", f"conv={row.conv}"], vars: {LC_ALL: "en_US.iso8859-1"})?
      uu.succeeds(r)
      assert uu.read(s, f"output-{row.target}")? == uu.read(s, f"input-{row.target}")?
    }
  }
}

# origin: gnu dd/direct.log
test test_gnu_dd_direct_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "in")?
  uu.truncate(s, "in", 8192)?
  let probe = uu.invoke(s, "dd", ["if=in", "oflag=direct", "of=out"])?
  if probe.status != 0 { test.skip("filesystem does not support aligned direct I/O") }
  for row in [{name: "short", size: 511}, {name: "m1", size: 8191}, {name: "p1", size: 8193}] {
    uu.touch(s, row.name)?
    uu.truncate(s, row.name, row.size)?
    uu.remove(s, "out")?
    uu.succeeds(uu.invoke(s, "dd", [f"if={row.name}", "iflag=direct", "oflag=direct", "of=out"])?)
  }
}

# origin: gnu dd/partial-write.log
test test_gnu_dd_partial_write_log { |ctx|
  let s = uu.scene(ctx)?
  let r = shell(s, r"""ulimit -S -f 1024 || exit 99; trap '' XFSZ || exit 99; exec "$@"; """,
    uu.argv(s, "dd", [p"if=/dev/zero", p"of=f", p"bs=768K", p"count=2"])?)
  uu.fails_with_code(r, 1)
  assert uu.size(s, "f")? > 0
  uu.stderr_contains(r, "+1 records out")
}

# origin: gnu dd/skip-seek.log
test test_gnu_dd_skip_seek_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "0123456789abcdef")?
  for row in [
    {args: ["bs=1", "skip=1", "seek=2", "conv=notrunc", "count=3"], out: "zy123utsrqponmlkji", err: "3+0 records in\n3+0 records out\n"},
    {args: ["bs=5", "skip=1", "seek=1", "conv=notrunc", "count=1"], out: "zyxwv56789ponmlkji", err: "1+0 records in\n1+0 records out\n"},
    {args: ["bs=5", "skip=1", "seek=1", "count=1"], out: "zyxwv56789", err: "1+0 records in\n1+0 records out\n"},
    {args: ["bs=1", "iseek=1", "oseek=2", "conv=notrunc", "count=3"], out: "zy123utsrqponmlkji", err: "3+0 records in\n3+0 records out\n"},
  ] {
    uu.write(s, "aux", "zyxwvutsrqponmlkji")?
    let r = uu.invoke_from_path(s, "dd", row.args.extend(["status=noxfer", "of=aux"]), uu.at(s, "in"))?
    uu.succeeds(r)
    uu.stderr_only(r, row.err)
    uu.file_is(s, "aux", row.out)
  }
  uu.write(s, "in", "01234567\nabcdefghijkl\n")?
  let block = uu.invoke_from_path(s, "dd", ["ibs=10", "cbs=10", "status=noxfer", "conv=block,sync"], uu.at(s, "in"))?
  uu.succeeds(block)
  uu.stdout_is(block, "01234567  abcdefghij          ")
  uu.stderr_is(block, "2+1 records in\n0+1 records out\n1 truncated record\n")
  let pipe = uu.invoke(s, "dd", ["bs=1", "skip=1", "status=noxfer"], stdin: b"abc\n")?
  uu.succeeds(pipe)
  uu.stdout_is(pipe, "bc\n")
  uu.stderr_is(pipe, "3+0 records in\n3+0 records out\n")
}

# origin: gnu dd/unblock-sync.log
test test_gnu_dd_unblock_sync_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "000100020003xx")?
  let r = uu.invoke_from_path(s, "dd", ["cbs=4", "ibs=4", "conv=unblock,sync"], uu.at(s, "in"))?
  uu.succeeds(r)
  uu.stdout_is(r, "0001\n0002\n0003\nxx\n")
}

# origin: gnu dd/unblock.log
test test_gnu_dd_unblock_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [
    {input: "", output: ""},
    {input: "a\n  ", output: "a\n\n\n"},
    {input: "a\n ", output: "a\n\n"},
    {input: "a  ", output: "a\n"},
    {input: "a \n ", output: "a \n\n\n"},
    {input: "a \n", output: "a \n\n"},
    {input: "a   ", output: "a\n\n"},
    {input: "a  \n", output: "a\n\n\n"},
  ] {
    uu.write(s, "in", row.input)?
    let r = uu.invoke_from_path(s, "dd", ["cbs=3", "conv=unblock", "status=noxfer"], uu.at(s, "in"))?
    uu.succeeds(r)
    uu.stdout_is(r, row.output)
    let lines = r.stderr.utf8()?.split("\n")
    assert lines.len() == 3 and rx"^[0-9]+\+[0-9]+ records in$".matches(lines[0]) and rx"^[0-9]+\+[0-9]+ records out$".matches(lines[1]) and lines[2] == ""
  }
}

# origin: gnu dd/not-rewound.log
test test_gnu_dd_not_rewound_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "abcde\n")?
  let r = sequential(s, ["skip=1", "count=1", "bs=1"], ["skip=1", "bs=1"], uu.at(s, "in"))
  uu.succeeds(r)
  uu.stdout_is(r, "bde\n")
}

# origin: gnu dd/skip-seek2.log
test test_gnu_dd_skip_seek2_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [
    {args: ["bs=5"], expected: "3456789abcdef\n"},
    {args: ["bs=5", "count=2"], expected: "3456789abc"},
  ] {
    uu.write(s, "in", "LA:3456789abcdef\n")?
    let r = sequential(s, ["bs=1", "skip=3", "count=0"], row.args, uu.at(s, "in"))
    uu.succeeds(r)
    uu.stdout_is(r, row.expected)
  }
}

# origin: gnu dd/stderr.log
test test_gnu_dd_stderr_log { |ctx|
  let s = uu.scene(ctx)?
  let help_words = uu.argv(s, "dd", [p"--help"])?
  let closing = [p"/bin/sh", p"-c", Path(r"""exec "$@" 2>&-; """), p"dd-stderr"]
  let help = shell(s, r"""exec "$@"; """, help_words[..3].extend(closing).extend(help_words[3..]))
  uu.succeeds(help)
  let closed_words = uu.argv(s, "dd", [])?
  let closed = shell(s, r"""exec "$@"; """, closed_words[..3].extend(closing).extend(closed_words[3..]))
  let full = uu.invoke(s, "dd", [], stderr: p"/dev/full")?
  assert closed.status == 1 and full.status == 1, f"stderr failures: closed={closed.status}, full={full.status}"
}

# origin: gnu dd/bytes.log
test test_gnu_dd_bytes_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "0123456789abcdefghijklm\n")?
  for options in [["count=14B"], ["count=14", "iflag=count_bytes"]] {
    let r = uu.invoke_from_path(s, "dd", options.extend(["conv=swab"]), uu.at(s, "in"))?
    uu.succeeds(r)
    uu.stdout_is(r, "1032547698badc")
  }
  for options in [["iseek=10B"], ["skip=10", "iflag=skip_bytes"]] {
    let file = uu.invoke_from_path(s, "dd", options, uu.at(s, "in"))?
    uu.succeeds(file)
    uu.stdout_is(file, "abcdefghijklm\n")
    let pipe = uu.invoke(s, "dd", options.extend(["bs=2"]), stdin: b"0123456789abcdefghijklm\n")?
    uu.succeeds(pipe)
    uu.stdout_is(pipe, "abcdefghijklm\n")
  }
  let expected = b"\0\0\0\0\0\0\0\0abcdefghijklm\n"
  for options in [["oseek=8B"], ["seek=8", "oflag=seek_bytes"]] {
    let r = uu.invoke(s, "dd", options.extend(["bs=5"]), stdin: b"abcdefghijklm\n")?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, expected)
    let empty = uu.invoke(s, "dd", options.extend(["bs=5", "of=out2", "count=0"]))?
    uu.succeeds(empty)
    assert uu.read(s, "out2")? == b"\0\0\0\0\0\0\0\0"
  }
  for options in [["oseek=1x2x4", "oflag=seek_bytes"], ["oseek=1Bx2x4"], ["oseek=1Bx8"], ["oseek=2Bx4B"], ["oseek=2x4B"]] {
    let r = uu.invoke(s, "dd", options.extend(["bs=5"]), stdin: b"abcdefghijklm\n")?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, expected)
  }
  let multiplier = ["1x" for _ in range(10000)].join("") + "1"
  let bounded = shell(s, r"""ulimit -S -s 256 || exit 99; exec "$@"; """,
    uu.argv(s, "dd", [Path(f"count={multiplier}"), p"if=/dev/null", p"of=/dev/null", p"status=none"])?)
  uu.succeeds(bounded)
  for count in ["B", "B1", "Bx1", "KBB", "BB", "KBb", "KBx", "x1", "1x", "1xx1"] {
    uu.fails_with_code(uu.invoke(s, "dd", [f"count={count}"])?, 1)
  }
}

# origin: gnu dd/skip-seek-past-file.log
test test_gnu_dd_skip_seek_past_file_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file", "1234")?
  let warning = "dd: 'standard input': cannot skip to specified offset\n0+0 records in\n0+0 records out\n"
  for options in [["bs=1", "skip=5"], ["bs=3", "skip=2"]] {
    let args = options.extend(["count=0", "status=noxfer"])
    let file = uu.invoke_from_path(s, "dd", args, uu.at(s, "file"))?
    uu.succeeds(file)
    uu.stderr_is(file, warning)
    let pipe = uu.invoke(s, "dd", args, stdin: b"1234")?
    uu.succeeds(pipe)
    uu.stderr_is(pipe, warning)
  }
  for options in [["bs=1", "skip=4", "status=noxfer"], ["bs=2", "skip=2", "status=noxfer"]] {
    let r = sequential(s, ["bs=1", "skip=1", "count=0"], options, uu.at(s, "file"), quiet_first: true)
    uu.succeeds(r)
    uu.stderr_is(r, warning)
  }
  let seek = uu.invoke(s, "dd", ["bs=1", "seek=5", "count=0", "status=noxfer"], stdout: uu.at(s, "file"))?
  uu.succeeds(seek)
  uu.stderr_is(seek, "0+0 records in\n0+0 records out\n")
  for skip in ["9223372036854775808", "9223372036854775808x9223372036854775808"] {
    let r = uu.invoke_from_path(s, "dd", ["bs=1", f"skip={skip}", "count=0", "status=noxfer"], uu.at(s, "file"))?
    uu.fails(r)
    uu.stderr_contains(r, "invalid number:")
  }
  let limit = uu.invoke_from_path(s, "dd", ["bs=1", "skip=9223372036854775807", "count=0", "status=none"], uu.at(s, "file"))?
  if limit.status == 0 { uu.no_stderr(limit) } else { uu.stderr_contains(limit, "cannot skip:") }
}

# origin: gnu dd/ascii.log
test test_gnu_dd_ascii_log { |ctx|
  let s = uu.scene(ctx)?
  let input = bytes.concat([b"\x40\xc1\x40\xc1\x40\xc1\x40\x40", bytes.from_ints([n for n in range(256)])?])
  uu.write_bytes(s, "in", input)?
  let expected = b"\x20\x41\x20\x41\x0a\x20\x41\x0a\x00\x01\x02\x03\x0a\x9c\x09\x86\x7f\x0a\x97\x8d\x8e\x0b\x0a\x0c\x0d\x0e\x0f\x0a\x10\x11\x12\x13\x0a\x9d\x85\x08\x87\x0a\x18\x19\x92\x8f\x0a\x1c\x1d\x1e\x1f\x0a\x80\x81\x82\x83\x0a\x84\x0a\x17\x1b\x0a\x88\x89\x8a\x8b\x0a\x8c\x05\x06\x07\x0a\x90\x91\x16\x93\x0a\x94\x95\x96\x04\x0a\x98\x99\x9a\x9b\x0a\x14\x15\x9e\x1a\x0a\x20\xa0\xa1\xa2\x0a\xa3\xa4\xa5\xa6\x0a\xa7\xa8\xd5\x2e\x0a\x3c\x28\x2b\x7c\x0a\x26\xa9\xaa\xab\x0a\xac\xad\xae\xaf\x0a\xb0\xb1\x21\x24\x0a\x2a\x29\x3b\x7e\x0a\x2d\x2f\xb2\xb3\x0a\xb4\xb5\xb6\xb7\x0a\xb8\xb9\xcb\x2c\x0a\x25\x5f\x3e\x3f\x0a\xba\xbb\xbc\xbd\x0a\xbe\xbf\xc0\xc1\x0a\xc2\x60\x3a\x23\x0a\x40\x27\x3d\x22\x0a\xc3\x61\x62\x63\x0a\x64\x65\x66\x67\x0a\x68\x69\xc4\xc5\x0a\xc6\xc7\xc8\xc9\x0a\xca\x6a\x6b\x6c\x0a\x6d\x6e\x6f\x70\x0a\x71\x72\x5e\xcc\x0a\xcd\xce\xcf\xd0\x0a\xd1\xe5\x73\x74\x0a\x75\x76\x77\x78\x0a\x79\x7a\xd2\xd3\x0a\xd4\x5b\xd6\xd7\x0a\xd8\xd9\xda\xdb\x0a\xdc\xdd\xde\xdf\x0a\xe0\xe1\xe2\xe3\x0a\xe4\x5d\xe6\xe7\x0a\x7b\x41\x42\x43\x0a\x44\x45\x46\x47\x0a\x48\x49\xe8\xe9\x0a\xea\xeb\xec\xed\x0a\x7d\x4a\x4b\x4c\x0a\x4d\x4e\x4f\x50\x0a\x51\x52\xee\xef\x0a\xf0\xf1\xf2\xf3\x0a\x5c\x9f\x53\x54\x0a\x55\x56\x57\x58\x0a\x59\x5a\xf4\xf5\x0a\xf6\xf7\xf8\xf9\x0a\x30\x31\x32\x33\x0a\x34\x35\x36\x37\x0a\x38\x39\xfa\xfb\x0a\xfc\xfd\xfe\xff\x0a"
  let r = uu.invoke(s, "dd", ["if=in", "of=out", "conv=ascii", "cbs=4"])?
  uu.succeeds(r)
  assert uu.read(s, "out")? == expected
}

# origin: gnu dd/reblock.log
test test_gnu_dd_reblock_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "dd.fifo")?
  for row in [
    {args: ["ibs=3", "obs=3", "if=dd.fifo"], expected: "0+2 records in\n1+1 records out\n4 bytes copied"},
    {args: ["bs=3", "ibs=1", "obs=1", "if=dd.fifo"], expected: "0+2 records in\n0+2 records out\n4 bytes copied"},
  ] {
    var matched = false
    for delay in ["0.1", "0.2", "0.4", "0.8", "1.6", "3.2"] {
      let out = uu.at(s, "out")
      let err = uu.at(s, "err")
      let plan = uu.command(s, "dd", row.args, stdout: out, stderr: err, timeout: 10s)?
      let consumer = spawn plan?
      let writer = [p"/bin/sh", p"-c", Path(r"""exec 3> "$1"; printf '%s' ab >&3; sleep "$2"; printf '%s' cd >&3; """), p"dd-producer", uu.at(s, "dd.fifo"), Path(delay)]
      assert process.run(process.command_argv(p"/bin/sh", writer, s.root, timeout: 10s))?.exited_with(0)
      assert (wait consumer?).exited_with(0)
      let lines = err.read_text()?.split("\n")
      let normalized = lines[0] + "\n" + lines[1] + "\n" + lines[2].split(",")[0]
      if normalized == row.expected { matched = true; break }
    }
    assert matched, row.expected
  }
}

# origin: gnu dd/sparse.log
test test_gnu_dd_sparse_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "sparse")?
  uu.truncate(s, "sparse", 1048576)?
  uu.succeeds(uu.invoke(s, "dd", ["bs=32K", "if=sparse", "of=sparse.dd", "conv=sparse"])?)
  assert uu.size(s, "sparse")? == uu.size(s, "sparse.dd")?
  uu.write_bytes(s, "file.in", b"a\0\0b")?
  let append = uu.invoke(s, "dd", ["if=file.in", "bs=1", "conv=sparse", "oflag=append"], stdout: uu.at(s, "out"))?
  uu.succeeds(append)
  assert uu.read(s, "out")? == b"ab"
  uu.write(s, "out", "____")?
  uu.succeeds(uu.invoke(s, "dd", ["if=file.in", "bs=1", "conv=sparse,notrunc", "of=out"])?)
  uu.file_is(s, "out", "a__b")
  let pipe = shell(s, r""""$@" | cat; """, uu.argv(s, "dd", [p"if=file.in", p"bs=1", p"conv=sparse"])?)
  uu.succeeds(pipe)
  uu.stdout_is_bytes(pipe, b"a\0\0b")
  uu.remove(s, "file.in")?
  uu.succeeds(uu.invoke(s, "dd", ["if=/dev/urandom", "of=file.in", "bs=1M", "count=3", "iflag=fullblock"])?)
  uu.succeeds(uu.invoke(s, "dd", ["if=/dev/zero", "of=file.in", "bs=1M", "count=1", "seek=1", "conv=notrunc"])?)
  if fs.stat(uu.at(s, "file.in"))?.blocks_512 / 2 > 3000 {
    uu.succeeds(uu.invoke(s, "dd", ["if=file.in", "of=file.out", "ibs=1M", "obs=2M", "conv=sparse"])?)
    let flushed = run.capture --text sync fp"{s.root}/file.out"
    assert flushed.status.exited_with(0)
    assert fs.stat(uu.at(s, "file.out"))?.blocks_512 / 2 > 2500
    uu.remove(s, "file.out")?
    uu.touch(s, "file.out")?
    uu.truncate(s, "file.out", 3145728)?
    uu.succeeds(uu.invoke(s, "dd", ["if=file.in", "of=file.out", "ibs=2M", "obs=1M", "conv=sparse,notrunc"])?)
    if fs.stat(uu.at(s, "file.out"))?.blocks_512 / 2 >= 2500 {
      uu.succeeds(uu.invoke(s, "dd", ["if=file.in", "of=manual.out", "bs=1M", "count=1"])?)
      uu.succeeds(uu.invoke(s, "dd", ["if=file.in", "of=manual.out", "bs=1M", "count=1", "seek=2", "conv=notrunc"])?)
      assert fs.stat(uu.at(s, "file.out"))?.blocks_512 / 2 == fs.stat(uu.at(s, "manual.out"))?.blocks_512 / 2
    }
  }
}
