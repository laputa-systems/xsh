use support.uu

proc sequence(count: Int) -> Bytes { bytes.from_text([f"{n}\n" for n in range(1, count + 1)].join("")) }
proc integers(data: Bytes) [error] -> Result[List[Int], Error] { Ok([word.parse_int()? for word in data.utf8()?.lines()] |> sort-by .) }
proc distinct(values: List[Str]) -> List[Str] {
  var result: List[Str] = []
  for value in values |> sort-by . { if value not in result { result += [value] } }
  result
}
# The isolation launcher runs before the wrapper so descriptor and address-space
# changes apply to the applet, rather than the launcher's own preparation.
proc wrapped(s: uu.Scene, prefix: List[Path], args: List[Str], input: Bytes = b"", timeout: Duration = 30s) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let isolated = uu.argv(s, "shuf", [Path(arg) for arg in args])?
  let words = isolated[..3].extend(prefix).extend(isolated[3..])
  let out = uu.at(s, "wrapped-out")
  let err = uu.at(s, "wrapped-err")
  let plan = process.command_argv(words[0], words, s.root, {}, input, out, err, timeout: timeout)
  let status = process.run(plan)?
  Ok({util: "shuf", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}
proc limited(s: uu.Scene, limit: Int, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  wrapped(s, [p"/bin/sh", p"-c", p"trap '' SEGV; ulimit -c 0; ulimit -v \"$1\" || exit 77; shift; exec \"$@\"", p"vm", Path(f"{limit}")], args)
}
proc minimum_vm(s: uu.Scene) [fs, process, env, time, error] -> Result[Int?, Error] {
  for level in range(1, 11) {
    let upper = level * 5000
    let started = time.now() / 1000
    let probe = limited(s, upper, ["-i1-1"])?
    if time.now() / 1000 - started >= 10 { return Ok(null) }
    if probe.status == 77 { return Ok(null) }
    if probe.status == 0 {
      var previous = upper
      for step in range(upper / 1000 - 1) {
        let lower = upper - 1000 * (step + 1)
        let descending_started = time.now() / 1000
        let descending = limited(s, lower, ["-i1-1"])?
        if time.now() / 1000 - descending_started >= 10 { return Ok(null) }
        if descending.status == 77 { return Ok(null) }
        if descending.status != 0 { return Ok(previous + 4) }
        previous = lower
      }
    }
  }
  Ok(null)
}
proc first_thousand(s: uu.Scene, args: List[Str], input: Bytes = b"") [fs, process, env, error] {
  let r = wrapped(s, [p"/bin/sh", p"-c", p"\"$@\" | head -n 1000", p"first-lines"], args, input)
  let completed = r?
  uu.succeeds(completed)
  assert completed.stdout.count_lines() == 1000
}
# origin: gnu shuf/shuf.log
test test_gnu_shuf_shuf_log { |ctx|
  let s = uu.scene(ctx)?
  let traced = wrapped(s, [p"strace", p"-o", p"/dev/null", p"-e", p"inject=getrandom:error=ENOSYS"], ["-i", "1-9"])?
  assert traced.status == 0 or traced.status == 1
  let first = wrapped(s, [p"setarch", p"-R"], ["-i", "1-18446744073709551615", "-n", "1"])?
  if first.status == 0 {
    let second = wrapped(s, [p"setarch", p"-R"], ["-i", "1-18446744073709551615", "-n", "1"])?
    if second.status == 0 { assert first.stdout != second.stdout }
  }
  let input = sequence(100)
  uu.write_bytes(s, "in", input)?
  for args in [["in"], ["-i", "1-100"]] {
    let r = uu.invoke(s, "shuf", args)?
    uu.succeeds(r)
    assert r.stdout != input
    assert integers(r.stdout)? == [n for n in range(1, 101)]
  }
  let closed = [p"/bin/sh", p"-c", p"exec 0<&-; exec \"$@\"", p"closed-input"]
  let zero_closed = wrapped(s, closed, ["-r", "-n", "0", "in"])?
  uu.succeeds(zero_closed)
  uu.no_stdout(zero_closed)
  let echoes = uu.invoke(s, "shuf", ["-e", "a", "b", "c", "d", "e"])?
  uu.succeeds(echoes)
  assert (echoes.stdout.utf8()?.lines() |> sort-by .) == ["a", "b", "c", "d", "e"]
  uu.fails_with_code(uu.invoke(s, "shuf", ["-er"])?, 1)
  uu.fails_with_code(uu.invoke(s, "shuf", ["-i0-0", "1"])?, 1)
  uu.succeeds(uu.invoke(s, "shuf", [], sequence(1860), stdout: p"/dev/null")?)
  let nul = uu.invoke(s, "shuf", ["--zero-terminated", "-i", "1-1"])?
  uu.succeeds(nul)
  uu.stdout_is_bytes(nul, b"1\0")
  if let vm = minimum_vm(s)? {
    # This deliberately invokes only the ulimit builtin with trailing words:
    # shells differ in whether they reject or ignore those extra operands.
    let guard_words = [p"/bin/sh", p"-c", p"ulimit -v \"$@\"", p"vm-guard", Path(f"{vm}"), p"shuf", p"-i1-1"]
    let guard_out = uu.at(s, "guard-out")
    let guard_err = uu.at(s, "guard-err")
    let guard_status = process.run(process.command_argv(guard_words[0], guard_words, s.root, {}, b"", guard_out, guard_err))?
    if guard_status.exited_with(0) { uu.fails_with_code(limited(s, vm, ["-i1-18446744073709551615"])?, 1) }
  }
  let subset = uu.invoke(s, "shuf", ["-i1-18446744073709551615", "-n2"], stdout: p"/dev/null", timeout: 10s)?
  uu.succeeds(subset)
  uu.touch(s, "unreadable")?
  uu.set_mode(s, "unreadable", 0)?
  uu.succeeds(uu.invoke(s, "shuf", ["-n0", "unreadable"])?)
  uu.fails_with_code(uu.invoke(s, "shuf", ["-n1", "unreadable"])?, 1)
  let multiple_counts = uu.invoke(s, "shuf", ["-n10", "-i0-9", "-n3", "-n20"])?
  uu.succeeds(multiple_counts)
  assert multiple_counts.stdout.count_lines() == 3
  var decimal = "18446744073709551615"
  while decimal != "" {
    if decimal.starts_with("0") { decimal = "1" + decimal.byte_slice(1) }
    let r = uu.invoke(s, "shuf", ["-i", f"{decimal}-{decimal}"])?
    uu.succeeds(r)
    uu.stdout_is(r, f"{decimal}\n")
    decimal = decimal.byte_slice(1)
  }
  let invalid = [
    ["-i0-9", "-e", "A", "B"],
    ["-nA"],
    ["-i0-9", "-n10", "-i8-90"],
    ["-i1"], ["-iA"], ["-i1-"], ["-i1-A"],
    ["-i0-9", "-o", "A", "-o", "B"],
    ["-i0-9", "--random-source", "A", "--random-source", "B"],
  ]
  for args in invalid { uu.fails_with_code(uu.invoke(s, "shuf", args)?, 1) }
  for args in [["-o", "out", "missing-input"], ["-o", "out", "--random-source=missing-input", "in"]] {
    uu.write(s, "out", "precious\n")?
    uu.fails_with_code(uu.invoke(s, "shuf", args)?, 1)
    uu.file_is(s, "out", "precious\n")
  }
  first_thousand(s, ["--rep", "-i", "0-10"])
  let repeated = uu.invoke(s, "shuf", ["--rep", "-i0-9", "-n1000"])?
  uu.succeeds(repeated)
  assert repeated.stdout.count_lines() == 1000
  assert distinct(repeated.stdout.utf8()?.lines()) == [f"{n}" for n in range(10)]
  let offset = uu.invoke(s, "shuf", ["--rep", "-i222-233", "-n2000"])?
  uu.succeeds(offset)
  assert distinct(offset.stdout.utf8()?.lines()) == [f"{n}" for n in range(222, 234)]
  let no_repeat = uu.invoke(s, "shuf", ["--rep", "-i0-9", "-n0"])?
  uu.succeeds(no_repeat)
  uu.no_stdout(no_repeat)
  first_thousand(s, ["--rep", "-e", "A", "B", "C", "D"])
  let letters = b"A\nB\nC\nD\nE\n"
  first_thousand(s, ["--rep"], letters)
  let repeated_input = uu.invoke(s, "shuf", ["--rep", "-n2000"], letters)?
  uu.succeeds(repeated_input)
  assert repeated_input.stdout.count_lines() == 2000
  assert distinct(repeated_input.stdout.utf8()?.lines()) == ["A", "B", "C", "D", "E"]
  let none_input = uu.invoke(s, "shuf", ["--rep", "-n0"], letters)?
  uu.succeeds(none_input)
  uu.no_stdout(none_input)
  let null_closed = wrapped(s, closed, ["/dev/null"])?
  uu.succeeds(null_closed)
  uu.no_stdout(null_closed)
}
