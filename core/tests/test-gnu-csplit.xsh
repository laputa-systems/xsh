use support.uu

pure piece_name(index: Int) -> Str {
  if index < 10 { f"xx0{index}" } else { f"xx{index}" }
}

proc clear_pieces(s: uu.Scene) [fs, error] -> Result[Unit, Error] {
  for entry in fs.children(s.root)? {
    if entry.name.starts_with("xx") { entry.path.remove()? }
  }
  Ok()
}

# origin: gnu csplit/csplit-1000.log
test test_gnu_csplit_csplit_1000_log { |ctx|
  let s = uu.scene(ctx)?
  let input = [f"{number}\n" for number in range(1, 1001)].join("")
  let r = uu.invoke(s, "csplit", ["-", "/./", "{*}"], stdin: bytes.from_text(input), timeout: 20s)?
  uu.succeeds(r)
  assert uu.exists(s, "xx1000")?
  assert ! uu.exists(s, "xx1001")?
}

# origin: gnu csplit/csplit-io-err.log
test test_gnu_csplit_csplit_io_err_log { |ctx|
  if ! p"/dev/full".exists()? { test.skip("requires /dev/full"); return }
  let s = uu.scene(ctx)?
  uu.symlink(s, "/dev/full", "xx01")?
  let full = uu.invoke(s, "csplit", ["-", "1"], stdin: b"1\n2\n")?
  uu.fails_with_code(full, 1)
  uu.stderr_is(full, "csplit: xx01: No space left on device\n")
  assert ! uu.exists(s, "xx01")?
  uu.mkdir(s, "xx01")?
  let directory = uu.invoke(s, "csplit", ["-", "1"], stdin: b"1\n2\n")?
  uu.fails_with_code(directory, 1)
  assert uu.dir_exists(s, "xx01")?
}

# origin: gnu csplit/csplit-suppress-matched.log
test test_gnu_csplit_csplit_suppress_matched_log { |ctx|
  let s = uu.scene(ctx)?
  let paragraphs = "a\na\nYY\n\nXX\nb\nb\nYY\n\nXX\nc\nYY\n\nXX\nd\nd\nd\n"
  let numbers = "1\n2\n3\n4\n5\n6\n"
  for item in [
    {args: ["-q", "-", "/^$/", "{*}"], input: paragraphs, outputs: ["a\na\nYY\n", "\nXX\nb\nb\nYY\n", "\nXX\nc\nYY\n", "\nXX\nd\nd\nd\n"]},
    {args: ["--suppress-matched", "-q", "-", "/^$/", "{*}"], input: paragraphs, outputs: ["a\na\nYY\n", "XX\nb\nb\nYY\n", "XX\nc\nYY\n", "XX\nd\nd\nd\n"]},
    {args: ["--suppress-matched", "-q", "-", "/^$/1", "{*}"], input: paragraphs, outputs: ["a\na\nYY\n\n", "b\nb\nYY\n\n", "c\nYY\n\n", "d\nd\nd\n"]},
    {args: ["--suppress-matched", "-q", "-", "/^$/-1", "{*}"], input: paragraphs, outputs: ["a\na\n", "\nXX\nb\nb\n", "\nXX\nc\n", "\nXX\nd\nd\nd\n"]},
    {args: ["--suppress-matched", "-q", "-", "/^$/", "{2}"], input: paragraphs, outputs: ["a\na\nYY\n", "XX\nb\nb\nYY\n", "XX\nc\nYY\n", "XX\nd\nd\nd\n"]},
    {args: ["-q", "-", "/^$/", "{*}"], input: "a\n\n\nb\n", outputs: ["a\n", "\n", "\nb\n"]},
    {args: ["--suppress-match", "-q", "-", "/^$/", "{*}"], input: "a\n\n\nb\n", outputs: ["a\n", "", "b\n"]},
    {args: ["--suppress-match", "-zq", "-", "/^$/", "{*}"], input: "a\n\n\nb\n", outputs: ["a\n", "b\n"]},
    {args: ["-q", "-", "/^$/", "{*}"], input: "a\n\nb\n\n", outputs: ["a\n", "\nb\n", "\n"]},
    {args: ["--suppress-match", "-q", "-", "/^$/", "{*}"], input: "a\n\nb\n\n", outputs: ["a\n", "b\n", ""]},
    {args: ["--suppress-match", "-zq", "-", "/^$/", "{*}"], input: "a\n\nb\n\n", outputs: ["a\n", "b\n"]},
    {args: ["-q", "-", "2", "4", "6"], input: numbers, outputs: ["1\n", "2\n3\n", "4\n5\n", "6\n"]},
    {args: ["--suppress-matched", "-q", "-", "2", "4", "6"], input: numbers, outputs: ["1\n", "3\n", "5\n", ""]},
    {args: ["--suppress-matched", "-zq", "-", "2", "4", "6"], input: numbers, outputs: ["1\n", "3\n", "5\n"]},
  ] {
    clear_pieces(s)?
    let r = uu.invoke(s, "csplit", item.args, stdin: bytes.from_text(item.input))?
    uu.succeeds(r)
    uu.no_output(r)
    for index in range(item.outputs.len()) { uu.file_is(s, piece_name(index), item.outputs[index]) }
    assert ! uu.exists(s, piece_name(item.outputs.len()))?
  }
}

# origin: gnu csplit/csplit.log
test test_gnu_csplit_csplit_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "a\n\n\n")?
  let blank = uu.invoke(s, "csplit", ["in", "/^$/", "2"])?
  uu.succeeds(blank)
  uu.stdout_is(blank, "2\n0\n2\n")
  uu.file_is(s, "xx02", "\n\n")

  uu.write(s, "in", "\na\n")?
  let negative = uu.invoke(s, "csplit", ["in", "/a/-1", "{*}"])?
  uu.succeeds(negative)
  uu.stdout_is(negative, "0\n3\n")

  uu.write(s, "in", "\n")?
  let repeated = uu.invoke(s, "csplit", ["in", "1", "1"])?
  uu.succeeds(repeated)
  uu.stdout_is(repeated, "0\n0\n1\n")
  uu.stderr_is(repeated, "csplit: warning: line number '1' is the same as preceding line number\n")

  let suffix = uu.invoke(s, "csplit", ["-b", "%0#6.3x", "in", "1"])?
  uu.succeeds(suffix)
  uu.stdout_only(suffix, "0\n1\n")
  uu.file_is(s, "xx   000", "")
  uu.file_is(s, "xx 0x001", "\n")
  clear_pieces(s)?

  for item in [
    {args: ["in", "0"], error: "csplit: 0: line number must be greater than zero\n"},
    {args: ["in", "2", "1"], error: "csplit: line number '1' is smaller than preceding line number, 2\n"},
    {args: ["in", "3", "3"], error: "csplit: warning: line number '3' is the same as preceding line number\ncsplit: '3': line number out of range\n"},
  ] {
    let r = uu.invoke(s, "csplit", item.args)?
    uu.fails(r)
    uu.stderr_is(r, item.error)
  }
  clear_pieces(s)?

  let padding = [" " for _ in range(8198)].join("")
  let input = "x" + padding + "x\nx\n" + padding + "x\nx\n"
  uu.write(s, "in", input)?
  let large = uu.invoke(s, "csplit", ["in", r"/x\{1\}/", "{*}"])?
  uu.succeeds(large)
  let names = fs.children(s.root)? |> where .name.starts_with("xx") |> map .name |> sort
  let pieces = bytes.concat([uu.read(s, name)? for name in names])
  assert pieces == bytes.from_text(input)
  clear_pieces(s)?

  let empty = uu.invoke(s, "csplit", ["/dev/null", "1"])?
  uu.fails(empty)
  uu.stderr_is(empty, "csplit: '1': line number out of range\n")
  assert ! uu.exists(s, "xx00")?
  clear_pieces(s)?

  uu.write(s, "inp", "a\n")?
  let empty_regex = uu.invoke(s, "csplit", ["inp", "//"])?
  uu.succeeds(empty_regex)
  uu.stdout_only(empty_regex, "0\n2\n")
  uu.file_is(s, "xx00", "")
  uu.file_is(s, "xx01", "a\n")

  for options in [[], ["-k"]] {
    clear_pieces(s)?
    let directory = uu.invoke(s, "csplit", options.extend([".", "/^a/"]))?
    uu.fails_with_code(directory, 1)
    uu.stdout_is(directory, "0\n")
    uu.stderr_is(directory, "csplit: read error: Is a directory\n")
    if options.is_empty() { assert ! uu.exists(s, "xx00")? } else { uu.file_is(s, "xx00", "") }
  }
}
