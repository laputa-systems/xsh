use support.uu

# origin: gnu ptx/ptx-overrun.log
test test_gnu_ptx_ptx_overrun_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "ptx", [], stdin: bytes.from_text("012345678901234567890123456789🛠"), timeout: 10s)?)
  uu.succeeds(uu.invoke(s, "ptx", [], stdin: b"\xff|\xed\xba\xad|\xc2\x89|\xed\xa6\xbf\xed\xbf\xbf\n", timeout: 10s)?)
  let name = "01234567890123456789012345678901234567890123456789"
  uu.touch(s, name)?
  uu.touch(s, "empty")?
  let slash = "\\"
  for args in [["-F", slash, name], ["-S", f"foo{slash}", name], ["-W", f"bar{slash}{slash}{slash}", name]] {
    let r = uu.invoke(s, "ptx", args, timeout: 10s)?
    uu.succeeds(r)
    uu.no_stdout(r)
  }
  uu.write(s, "ws.in", "This is a ptx whitespace Trimming test\n")?
  let twice = uu.invoke(s, "ptx", ["ws.in", "ws.in"], timeout: 10s)?
  let lines = twice.stdout.utf8()?.lines()
  for line in lines {
    assert [candidate for candidate in lines if candidate == line].len() != 1
  }
  uu.write(s, "a", "a\n")?
  uu.succeeds(uu.invoke(s, "ptx", ["-w1", "-A", uu.at(s, "a").display()], timeout: 10s)?)
  uu.succeeds(uu.invoke(s, "ptx", ["-G", "-w2"], stdin: b"qux\n", timeout: 10s)?)
}

# origin: gnu ptx/word-regex-loop.log
test test_gnu_ptx_word_regex_loop_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "aa bb cc\n")?
  for pattern_word in ["a*", "[a-z]*", "[ab]*", "\\(a\\)*"] {
    uu.succeeds(uu.invoke(s, "ptx", ["-W", pattern_word, "in"], timeout: 10s)?)
  }
  let nullable = uu.invoke(s, "ptx", ["-W", "[ab]*", "in"], timeout: 10s)?
  let nonempty = uu.invoke(s, "ptx", ["-W", "[ab]+", "in"], timeout: 10s)?
  uu.succeeds(nullable)
  uu.succeeds(nonempty)
  assert nullable.stdout == nonempty.stdout
}
