# The cmp applet is checked against results recorded from GNU diffutils 3.12.
# `data/cmp/cases.txt` lists each case's arguments and the exact stdout, stderr
# and exit status GNU produced on the files under `data/cmp/fixtures`, so the
# suite needs no GNU cmp at test time.

type Ran = {status: Int, stdout: Bytes, stderr: Bytes}

# One recorded case: arguments, optional stdin fixture, locale, expected status
# and the stored stream lines.
type Recorded = {id: Str, args: List[Str], stdin: Str, locale: Str, status: Int, out: List[Str], err: List[Str]}

proc invoke(ctx: TestContext, args: List[Str], input = b"", cwd: Path? = null, label = "cmp-capture", locale = "C") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: label)?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/cmp.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), cwd ?? root, {LC_ALL: locale, TZ: "UTC", TERM: "xterm"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

pure hex_value(pair: Str) -> Int {
  let digits = "0123456789abcdef"
  (digits.find(pair.byte_slice(0, length: 1)) ?? 0) * 16 + (digits.find(pair.byte_slice(1, length: 1)) ?? 0)
}

# Decode the escapes of a stored line: `\\` and `\xHH`.
pure decode(text: Str) -> Bytes {
  if text.find("\\") == null { return bytes.from_text(text) }
  let raw = bytes.from_text(text)
  var values: List[Int] = []
  var at = 0
  while at < raw.len() {
    let value = raw.byte_at(at) ?? 0
    if value == 92 and raw.byte_at(at + 1) == 92 {
      values += [92]
      at += 2
    } else if value == 92 and raw.byte_at(at + 1) == 120 {
      values += [hex_value(raw[at + 2..at + 4].utf8() ?? "00")]
      at += 4
    } else {
      values += [value]
      at += 1
    }
  }
  bytes.from_ints(values) ?? b""
}

# The stream bytes of stored lines: a lower-case tag ends the line with a
# newline, an upper-case tag is a final line without one.
pure joined(lines: List[Str], small: Str) -> Bytes {
  var chunks: List[Bytes] = []
  for line in lines {
    chunks += [decode(line.byte_slice(2))]
    if line.starts_with(small) { chunks += [b"\n"] }
  }
  bytes.concat(chunks)
}

proc load(table: Path) [fs, error] -> Result[List[Recorded]] {
  var cases: List[Recorded] = []
  var current: Recorded? = null
  for line in table.lines()? {
    if line.starts_with("#") or line == "" { continue }
    if line.starts_with("@") {
      if let done = current { cases += [done] }
      let fields = line.byte_slice(1).split("\t")
      var arguments: List[Str] = []
      for field in fields[1..] { arguments += [decode(field).utf8() ?? ""] }
      current = {id: fields[0], args: arguments, stdin: "", locale: "C", status: 0, out: [], err: []}
      continue
    }
    let found = current ?? {id: "", args: [], stdin: "", locale: "C", status: 0, out: [], err: []}
    if line.starts_with("in ") {
      current = {...found, stdin: line.byte_slice(3)}
    } else if line.starts_with("lc ") {
      current = {...found, locale: line.byte_slice(3)}
    } else if line.starts_with("status ") {
      current = {...found, status: line.byte_slice(7).parse_int() ?? 0}
    } else if line.starts_with("o:") or line.starts_with("O:") {
      current = {...found, out: found.out + [line]}
    } else if line.starts_with("e:") or line.starts_with("E:") {
      current = {...found, err: found.err + [line]}
    }
  }
  if let done = current { cases += [done] }
  Ok(cases)
}

proc check(ctx: TestContext, root: Path, entry: Recorded) [fs, process, error] -> Str {
  let input = if entry.stdin == "" { b"" } else { fp"{root}/{entry.stdin}".read_bytes() ?? b"" }
  let result = invoke(ctx, entry.args, input, cwd: root, label: f"cmp-{entry.id}", locale: entry.locale)
  if let Err(failure) = result { return f"{entry.id}: could not run: {failure.message}" }
  let ran = result ?? {status: -1, stdout: b"", stderr: b""}
  let expected_out = joined(entry.out, "o:")
  let expected_err = joined(entry.err, "e:")
  if ran.status != entry.status or ran.stdout != expected_out or ran.stderr != expected_err {
    return f"{entry.id} (cmp {entry.args.join(" ")}): status {ran.status} want {entry.status}\nstdout got {ran.stdout.dump()}\nstdout want {expected_out.dump()}\nstderr got {ran.stderr.dump()}\nstderr want {expected_err.dump()}"
  }
  ""
}

test test_cmp_recorded_cases_match_gnu_cmp { |ctx|
  let root = test.temp_dir(ctx, name: "cmp-recorded")?
  let _ = fs.copy_tree(fp"{ctx.core_dir}/tests/data/cmp/fixtures", fp"{root}/fx")?
  let all = load(fp"{ctx.core_dir}/tests/data/cmp/cases.txt")?
  assert !all.is_empty()
  let verdicts = all |> par-map(jobs: 8) { |item| check(ctx, fp"{root}/fx", item) } |> collect()
  let failures = [text for text in verdicts if text != ""]
  let shown = if failures.len() < 3 { failures.len() } else { 3 }
  assert failures.is_empty(), f"{failures.len()} of {all.len()} cases differ from GNU cmp:\n" + failures[0..shown].join("\n\n")
}

# The tests below predate the recorded table and keep their names.
test test_cmp_byte_difference_status { |ctx|
  let left = test.temp_file(ctx, name: "left", contents: b"a\nb")?
  let right = test.temp_file(ctx, name: "right", contents: b"a\nc")?
  let result = invoke(ctx, [left.display(), right.display()])?
  assert result.status == 1
  assert result.stdout.utf8()?.ends_with("differ: char 3, line 2\n")
  assert invoke(ctx, ["-s", left.display(), right.display()])?.stdout == b""
  assert invoke(ctx, ["-n", "2", left.display(), right.display()])?.status == 0
}

test test_cmp_stdin_offsets_and_listing { |ctx|
  let file = test.temp_file(ctx, name: "right", contents: b"zab")?
  assert invoke(ctx, ["-i", "1:1", "-", file.display()], b"xab")?.status == 0
  let result = invoke(ctx, ["-l", "-", file.display()], b"yab")?
  assert result.status == 1
  assert result.stdout.utf8()?.trim() == "1 171 172"
}

test test_cmp_chunk_boundary_and_zero_limit_missing_file { |ctx|
  let left = test.temp_file(ctx, name: "large-left", contents: bytes.zero(65537)?)?
  let right = test.temp_file(ctx, name: "large-right", contents: bytes.zero(65537)?)?
  assert bytes.write_at(left, 65536, b"a")? == 1
  assert bytes.write_at(right, 65536, b"b")? == 1
  let result = invoke(ctx, [left.display(), right.display()])?
  assert result.status == 1
  assert result.stdout.utf8()?.ends_with("differ: char 65537, line 1\n"), result.stdout.utf8()?
  let missing = test.temp_path(ctx, name: "missing")
  assert invoke(ctx, ["-n0", missing.display(), right.display()])?.status == 2
}

test test_cmp_eof_is_difference_and_io_errors_are_trouble { |ctx|
  let file = test.temp_file(ctx, name: "longer", contents: b"abc")?
  let shorter = invoke(ctx, ["-", file.display()], b"ab")?
  assert shorter.status == 1
  assert shorter.stderr.utf8()?.find("EOF") != null
  let missing = test.temp_path(ctx, name: "missing")
  let failed = invoke(ctx, ["-s", missing.display(), file.display()])?
  assert failed.status == 2
  assert failed.stderr == b""
}

test test_cmp_directory_operand_is_a_read_error { |ctx|
  let root = test.temp_dir(ctx, name: "cmp-dir")?
  fp"{root}/d".mkdir()?
  fp"{root}/f".write("x\n")?
  let result = invoke(ctx, ["d", "f"], cwd: root)?
  assert result.status == 2
  assert result.stderr.utf8()? == "cmp: d: Is a directory\n", result.stderr.utf8()?
  assert invoke(ctx, ["d", "d"], cwd: root)?.status == 0
}

test test_cmp_unreadable_file_is_trouble_unless_silent { |ctx|
  if unix.id()?.euid == 0 {
    test.skip("root reads files regardless of their mode")
    return
  }
  let root = test.temp_dir(ctx, name: "cmp-unreadable")?
  let secret = fp"{root}/secret"
  secret.write("x\n")?
  secret.chmod(0o000)?
  fp"{root}/ok".write("x\n")?
  let loud = invoke(ctx, ["secret", "ok"], cwd: root)?
  assert loud.status == 2
  assert loud.stderr.utf8()? == "cmp: secret: Permission denied\n", loud.stderr.utf8()?
  let quiet = invoke(ctx, ["-s", "secret", "ok"], cwd: root)?
  assert quiet.status == 2
  assert quiet.stderr == b""
}
