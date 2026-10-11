use support.uu

# origin: gnu paste/multi-byte.log
test test_gnu_paste_multi_byte_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f1", "1\n2\n")?
  uu.write(s, "f2", "a\nb\n")?
  for delimiter in ["¢", "€", "😀"] {
    let r = uu.invoke(s, "paste", ["-d", delimiter, "f1", "f2"], vars: {LC_ALL: "fr_FR.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, bytes.from_text(f"1{delimiter}a\n2{delimiter}b\n"))
  }
  uu.write(s, "f3", "1\n2\n3\n")?
  for delimiter in ["¢", "€"] {
    let r = uu.invoke(s, "paste", ["-s", "-d", delimiter, "f3"], vars: {LC_ALL: "fr_FR.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, bytes.from_text(f"1{delimiter}2{delimiter}3\n"))
  }
  uu.write(s, "f4", "a\nb\nc\n")?
  uu.write(s, "f5", "1\n2\n3\n")?
  uu.write(s, "f6", "x\ny\nz\n")?
  let alternating = uu.invoke(s, "paste", ["-d", "¢€", "f4", "f5", "f6"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(alternating)
  uu.stdout_is_bytes(alternating, bytes.from_text("a¢1€x\nb¢2€y\nc¢3€z\n"))
  let empty = uu.invoke(s, "paste", ["-s", "-d", "€\\0", "f3"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(empty)
  uu.stdout_is_bytes(empty, bytes.from_text("1€23\n"))
  let invalid = Path.parse_bytes(b"\xff|\xed\xba\xad|\xc2\x89|\xed\xa6\xbf\xed\xbf\xbf")?
  let raw = uu.invoke_paths(s, "paste", [p"-d", invalid, p"f1", p"f2"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(raw)
  uu.stdout_is_bytes(raw, b"1\xffa\n2\xffb\n")
  let escaped = uu.invoke(s, "paste", ["-d", "\\€", "f1", "f2"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  let ordinary = uu.invoke(s, "paste", ["-d", "€", "f1", "f2"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(escaped)
  uu.succeeds(ordinary)
  assert escaped.stdout == ordinary.stdout
  for item in [
    {args: [p"-d", Path.parse_bytes(b"\xa2\xe3")?, p"f1", p"f2"], out: b"1\xa2\xe3a\n2\xa2\xe3b\n"},
    {args: [p"-s", p"-d", Path.parse_bytes(b"\xa2\xe3")?, p"f3"], out: b"1\xa2\xe32\xa2\xe33\n"},
    {args: [p"-d", Path.parse_bytes(b"\xff")?, p"f1", p"f2"], out: b"1\xffa\n2\xffb\n"},
  ] {
    let r = uu.invoke_paths(s, "paste", item.args, vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, item.out)
  }
}

# origin: gnu paste/paste.log
test test_gnu_paste_paste_log { |ctx|
  let s = uu.scene(ctx)?
  for terminator in ["\n", "\0"] {
    for two_rows in [false, true] {
      for ends in [{left: "", right: ""}, {left: terminator, right: ""}, {left: "", right: terminator}, {left: terminator, right: terminator}] {
        let left = if two_rows { "1" + terminator + "a" } else { "a" }
        let right = if two_rows { "2" + terminator + "b" } else { "b" }
        uu.write(s, "left", left + ends.left)?
        uu.write(s, "right", right + ends.right)?
        let flags = if two_rows { if terminator == "\0" { ["-zd "] } else { ["-d "] } } else { if terminator == "\0" { ["-z"] } else { [] } }
        let expected = if two_rows { "1 2" + terminator + "a b" + terminator } else { "a\tb" + terminator }
        let r = uu.invoke(s, "paste", flags.extend(["left", "right"]))?
        uu.succeeds(r)
        uu.stdout_only_bytes(r, bytes.from_text(expected))
      }
    }
  }
  for name in [["a" for _ in range(50)].join(""), r"123\b\b\b.....@"] {
    uu.touch(s, name)?
    let r = uu.invoke(s, "paste", ["-d\\", name])?
    uu.fails_with_code(r, 1)
    uu.stderr_only(r, "paste: delimiter list ends with an unescaped backslash: \\\n")
  }
  uu.write(s, "left", "1\n2\n3\n")?
  let empty = uu.invoke(s, "paste", ["-s", "-d", r"\0,", "left"])?
  uu.succeeds(empty)
  uu.stdout_only(empty, "12,3\n")
  uu.write(s, "left", "0\n1\n")?
  uu.write(s, "middle", "2\n3\n4\n5\n6\n")?
  uu.write(s, "right", "7\n8\n9\n")?
  let reset = uu.invoke(s, "paste", ["-s", "-d", "abc", "left", "middle", "right"])?
  uu.succeeds(reset)
  uu.stdout_only(reset, "0a1\n2a3b4c5a6\n7a8b9\n")
  uu.write(s, "left", "1\n2\n")?
  for item in [
    {delimiter: r"\0", expected: "12\n"},
    {delimiter: r"\n", expected: "1\n2\n"},
    {delimiter: r"\t", expected: "1\t2\n"},
    {delimiter: r"\\", expected: "1\\2\n"},
    {delimiter: r"\b", expected: "1\u{8}2\n"},
    {delimiter: r"\f", expected: "1\u{c}2\n"},
    {delimiter: r"\r", expected: "1\r2\n"},
    {delimiter: r"\v", expected: "1\u{b}2\n"},
    {delimiter: r"\q", expected: "1q2\n"},
  ] {
    let r = uu.invoke(s, "paste", ["-s", "-d", item.delimiter, "left"])?
    uu.succeeds(r)
    uu.stdout_only(r, item.expected)
  }
}
