use support.uu as uu

proc wrapped(s: uu.Scene, words: List[Path], input: Bytes = b"", stdin: Path? = null, timeout: Duration = 20s) [fs, process, env, error] -> uu.Ran {
  let out = uu.at(s, "wrapped-out")
  let err = uu.at(s, "wrapped-err")
  let plan = if let input_path = stdin {
    process.command_argv(words[0], words, s.root, {LC_ALL: "C"}, input_path, out, err, timeout: timeout)
  } else {
    process.command_argv(words[0], words, s.root, {LC_ALL: "C"}, input, out, err, timeout: timeout)
  }
  let status = process.run(plan)?.exit_code()?
  {util: "od", args: [], status: status, stdout: out.read_bytes()?, stderr: err.read_bytes()?}
}

# origin: gnu od/big-w.log
test test_gnu_od_big_w_log { |ctx|
  test.timeout(ctx, 600s)
  let s = uu.scene(ctx)?
  for width in [46340, 46341, 3037000500, 3037000501] {
    let command_words = uu.argv(s, "od", [Path(f"-w{width}"), p"-tcz"])?
    let normalized = [p"/bin/sh", p"-c", Path(r""""$@" | tr -s ' ' ' '; """), p"od-wide"].extend(command_words)
    let r = wrapped(s, normalized, input: b"x", timeout: 240s)
    if r.stderr.len() > 0 {
      uu.no_stdout(r)
    } else {
      uu.stdout_is(r, "0000000 x >x<\n0000001\n")
      let counted = [p"/bin/sh", p"-c", Path(r""""$@" | wc -c; """), p"od-wide-count"].extend(command_words)
      let count = wrapped(s, counted, input: b"x", timeout: 240s)
      assert count.stdout.utf8()?.trim().parse_int()? == width * 4 + 21
    }
  }
}

# origin: gnu od/od-N.log
test test_gnu_od_od_N_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "abcdefg\n")?
  let one = uu.argv(s, "od", [p"-An", p"-N3", p"-c"])?
  let two = uu.argv(s, "od", [p"-An", p"-N3", p"-c"])?
  let left = ["\"" + r"$" + "{" + f"{index + 1}" + "}\"" for index in range(one.len())].join(" ")
  let right = ["\"" + r"$" + "{" + f"{index + one.len() + 1}" + "}\"" for index in range(two.len())].join(" ")
  let words = [p"/bin/sh", p"-c", Path(f"{left}; {right}"), p"od-shared"].extend(one).extend(two)
  let shared = wrapped(s, words, stdin: uu.at(s, "in"))
  uu.stdout_is(shared, "   a   b   c\n   d   e   f\n")
  let spaces100 = [" " for _ in range(100)].join("")
  let spaces10 = [" " for _ in range(10)].join("")
  for row in [
    {args: ["-N100", "-S1"], input: spaces100, out: "0000000 " + spaces100 + "\n"},
    {args: ["-N10", "-S10"], input: spaces100, out: "0000000 " + spaces10 + "\n"},
    {args: ["-N10", "-S1"], input: spaces100, out: "0000000 " + spaces10 + "\n"},
    {args: ["-N11", "-S11"], input: spaces10 + "\0", out: ""},
    {args: ["-S11"], input: spaces10 + "\0", out: ""},
    {args: ["-S10"], input: spaces10, out: ""},
    {args: ["-S10"], input: "\x01" + spaces10 + "\0" + spaces10 + "\0", out: "0000001 " + spaces10 + "\n0000014 " + spaces10 + "\n"},
  ] {
    let r = uu.invoke(s, "od", row.args, stdin: bytes.from_text(row.input))?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
  let directory = uu.invoke(s, "od", ["-N1", "."])?
  uu.fails_with_code(directory, 1)
  uu.no_stdout(directory)
}

# origin: gnu od/od-endian.log
test test_gnu_od_od_endian_log { |ctx|
  let s = uu.scene(ctx)?
  let original = b"0123456789abcdef"
  for endian in ["little", "big"] {
    let opposite = if endian == "little" { "big" } else { "little" }
    for width in [1, 2, 4, 8, 16] {
      var values: List[Int] = []
      for block in range(16 / width) {
        for offset in range(width) {
          if let value = original.byte_at(block * width + width - offset - 1) { values += [value] } else { assert false, "missing input byte" }
        }
      }
      let swapped = bytes.from_ints(values)?
      for format in ["x", "f"] {
        let probe = uu.invoke(s, "od", ["-t", f"{format}{width}", f"--endian={endian}", "/dev/null"])?
        if probe.status == 0 {
          let left = uu.invoke(s, "od", ["-An", "-t", f"{format}{width}", f"--endian={endian}"], stdin: original)?
          let right = uu.invoke(s, "od", ["-An", "-t", f"{format}{width}", f"--endian={opposite}"], stdin: swapped)?
          assert left.stdout == right.stdout
        }
      }
    }
  }
}

# origin: gnu od/od-j.log
test test_gnu_od_od_j_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "od", ["-j1", "/dev/null"])?)
  let regular = bytes.from_ints([if n % 80 == 79 { 10 } else { 97 + n % 26 } for n in range(24756)])?
  uu.write_bytes(s, "regular", regular)?
  for name in ["regular", "/proc/version", "/sys/kernel/profiling"] {
    let file = if name == "regular" { uu.at(s, name) } else { Path(name) }
    if let Ok(data) = file.read_bytes() {
      uu.write_bytes(s, "copy", data)?
      let expected = uu.invoke(s, "od", ["-An", name])?
      uu.succeeds(expected)
      let skipped = uu.invoke(s, "od", ["-An", "-j", f"{data.len()}", name, name])?
      uu.succeeds(skipped)
      assert skipped.stdout == expected.stdout
      let copy = uu.invoke(s, "od", ["-An", "-j", "4096", "copy", "copy"])?
      let source = uu.invoke(s, "od", ["-An", "-j", "4096", name, name])?
      assert source.status == copy.status
      assert source.stdout == copy.stdout
      assert source.stderr == copy.stderr
    }
  }
}

# origin: gnu od/od-multiple-t.log
test test_gnu_od_od_multiple_t_log { |ctx|
  let s = uu.scene(ctx)?
  let input = [f"{n}\n" for n in range(1, 20)].join("")
  assert input.byte_len() == 48
  uu.write(s, "in", input)?
  let formats = ["a", "c", "dC", "dS", "dI", "dL", "oC", "oS", "oI", "oL", "uC", "uS", "uI", "uL", "xC", "xS", "xI", "xL", "fF", "fD"]
  for first in formats { for second in formats {
    let r = uu.invoke(s, "od", ["-An", f"-t{first}z", f"-t{second}z", "in"])?
    uu.succeeds(r)
    let lines = r.stdout.utf8()?.split("\n")
    assert r.stdout.len() == (lines[0].byte_len() + 1) * (lines.len() - 1), f"{first} {second}"
  } }
}

# origin: gnu od/od-x8.log
test test_gnu_od_od_x8_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "od", ["-t", "x8", "/dev/null"])?)
  uu.write(s, "in", "abcdefgh")?
  let wide = uu.invoke(s, "od", ["-An", "-t", "x8", "in"])?
  uu.succeeds(wide)
  let narrow = uu.invoke(s, "od", ["-An", "-t", "x1", "in"])?
  let number = wide.stdout.utf8()?.trim()
  let parts = [number.byte_slice(n * 2, length: 2) for n in range(number.byte_len() / 2)] |> sort
  let expected = [word for word in narrow.stdout.utf8()?.trim().split(" ") if word != ""] |> sort
  assert parts == expected
}

# origin: gnu od/od-float.log
test test_gnu_od_od_float_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [
    {format: "fF", width: 4, obsolete: "-1.694740e+38"},
    {format: "fD", width: 8, obsolete: "-5.314010372517808e+303"},
    {format: "fL", width: 16, obsolete: "-1.023442870282055988e+4855"},
  ] {
    let values = [254 for _ in range(row.width)].extend([255]).extend([254 for _ in range(row.width - 1)]).extend([10])
    let r = uu.invoke(s, "od", ["-t", row.format], stdin: bytes.from_ints(values)?)?
    let text = r.stdout.utf8()?
    assert "0-" not in text
    let words = [word for word in text.replace("\n", with: " ").split(" ") if word != ""]
    if words.len() >= 3 { assert words[1] != row.obsolete or words[2] != row.obsolete }
  }
  let extended = uu.invoke(s, "od", ["-t", "fL"], stdin: b"\0\0\0\0\0\0\0\0\xff\xff\0\0\0\0\0\0")?
  assert extended.status == 0 or extended.status == 1, "extended float conversion must not crash"
  for format in ["-tfH", "-tf2"] {
    let half = uu.invoke(s, "od", ["--end=big", "-An", format], stdin: b"\x3c\0\x3c\0")?
    assert half.stdout.utf8()?.replace(" ", with: "").trim() == "11"
  }
  let brain = uu.invoke(s, "od", ["--end=big", "-An", "-tfB"], stdin: b"\x3f\x80\x3f\x80")?
  assert brain.stdout.utf8()?.replace(" ", with: "").trim() == "11"
  for row in [
    {format: "f", expected: "        2.000000473111868\n"},
    {format: "fD", expected: "        2.000000473111868\n"},
    {format: "fF", expected: "               1               2\n"},
  ] {
    let r = uu.invoke(s, "od", ["-An", "-t", row.format, "--endian=little"], stdin: b"\0\0\x80\x3f\0\0\0\x40")?
    uu.succeeds(r)
    uu.stdout_is(r, row.expected)
  }
}
