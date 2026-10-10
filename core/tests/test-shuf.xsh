type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/shuf.xsh by its real path inside `root`, capturing both streams.
proc shuf_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/shuf.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

# Like `shuf_run`, but each argument may be a Path built from raw bytes, so an
# argument that is not valid UTF-8 reaches the applet unchanged.
proc shuf_raw_run(ctx: TestContext, root: Path, args: List[Union[Str, Path]]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let words: List[Union[Str, Path]] = [ctx.xsh_bin, fp"{ctx.core_dir}/shuf.xsh"]
  let plan = process.command_argv(ctx.xsh_bin, words.extend(args), root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

pure sorted_lines(data: Bytes) -> List[Str] {
  [line for line in (data.utf8() ?? "").lines() |> sort-by .]
}

pure decimal_in_range(value: Str, lower: Str, upper: Str) -> Bool {
  return false when ! rx"^[0-9]+$".matches(value)
  return false when value.byte_len() < lower.byte_len() or value.byte_len() > upper.byte_len()
  return false when value.byte_len() == lower.byte_len() and value < lower
  return false when value.byte_len() == upper.byte_len() and value > upper
  true
}

test test_shuf_permutes_head_counts_and_repeats { |ctx|
  let root = test.temp_dir(ctx, name: "shuf")?
  fp"{root}/ten".write("1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n")

  let all = shuf_run(ctx, root, ["ten"])?
  assert all.status == 0
  assert sorted_lines(all.stdout) == ["1", "10", "2", "3", "4", "5", "6", "7", "8", "9"]
  assert sorted_lines(shuf_run(ctx, root, ["-i", "1-3"])?.stdout) == ["1", "2", "3"]
  assert shuf_run(ctx, root, ["-n", "4", "ten"])?.stdout.utf8()?.count_lines() == 4
  assert shuf_run(ctx, root, ["-n", "6", "-n", "3", "ten"])?.stdout.utf8()?.count_lines() == 3, "the smallest -n wins"
  assert shuf_run(ctx, root, ["-r", "-n", "50", "ten"])?.stdout.utf8()?.count_lines() == 50
  assert shuf_run(ctx, root, ["-n", "0", "ten"])?.stdout == b""
  assert shuf_run(ctx, root, ["-z", "-i", "1-3"])?.stdout.len() == 6, "-z ends records with NUL"
  assert sorted_lines(shuf_run(ctx, root, ["-e", "x", "y", "z"])?.stdout) == ["x", "y", "z"]
  assert shuf_run(ctx, root, ["-e", "-n", "2", "a\nb", "c\nd"])?.stdout.len() == 8, "arguments are not split into lines"
  assert shuf_run(ctx, root, ["-i", "5-4"])?.stdout == b""
  assert shuf_run(ctx, root, ["-i", "5-5", "-r", "-n", "1"])?.stdout == b"5\n"
}

test test_shuf_random_source_matches_gnu { |ctx|
  let root = test.temp_dir(ctx, name: "shuf")?
  fp"{root}/bytes".write(b"\xd1\xfd\xb9\x9a\xf5\x81qB\xf9zYy\xd4\x9c\x8c}")
  fp"{root}/seven".write("1\n2\n3\n4\n5\n6\n7\n")

  assert shuf_run(ctx, root, ["--random-source=bytes", "-e", "1", "2", "3", "4", "5", "6", "7"])?.stdout == b"7\n1\n2\n5\n3\n4\n6\n"
  assert shuf_run(ctx, root, ["--random-source=bytes", "seven"])?.stdout == b"7\n1\n2\n5\n3\n4\n6\n"
  assert shuf_run(ctx, root, ["--random-source=bytes", "-n", "5", "seven"])?.stdout == b"7\n1\n2\n5\n3\n"
  assert shuf_run(ctx, root, ["--random-source=bytes", "-n", "5"], b"1\n2\n3\n4\n5\n6\n7\n")?.stdout == b"5\n1\n4\n2\n3\n", "a pipe with -n goes through a reservoir"
  assert shuf_run(ctx, root, ["--random-source=bytes", "-n", "7"], b"1\n2\n3\n4\n5\n6\n7\n")?.stdout == b"6\n5\n1\n3\n2\n7\n4\n"
  assert shuf_run(ctx, root, ["--random-source=bytes", "-i", "1-10"])?.stdout == b"10\n2\n8\n7\n3\n9\n6\n5\n1\n4\n"

  fp"{root}/short".write(b"\xfb\x83\x8f!\x9b<-\xc5s\xa5XlT/Y\xf8")
  let exhausted = shuf_run(ctx, root, ["--random-source=short", "-r", "-i", "1-99"])?
  assert exhausted.status == 1
  assert exhausted.stdout == b"38\n30\n10\n26\n23\n61\n46\n99\n75\n43\n10\n89\n10\n44\n24\n59\n22\n51\n"
  assert exhausted.stderr == "shuf: end of random source\n", exhausted.stderr
}

test test_shuf_output_file_and_errors { |ctx|
  let root = test.temp_dir(ctx, name: "shuf")?
  fp"{root}/out".write("keep me\n")

  assert shuf_run(ctx, root, ["-n", "0", "-o", "out"])?.status == 0
  assert fp"{root}/out".read_bytes()? == b"", "-n 0 still truncates the output file"
  assert shuf_run(ctx, root, ["-i", "1-1", "-o", "out"])?.status == 0
  assert fp"{root}/out".read_bytes()? == b"1\n"

  fp"{root}/out".write("keep me\n")
  let missing = shuf_run(ctx, root, ["-o", "out", "does-not-exist"])?
  assert missing.status == 1
  assert missing.stderr == "shuf: does-not-exist: No such file or directory\n", missing.stderr
  assert fp"{root}/out".read_bytes()? == b"keep me\n", "a failed read leaves the output file alone"

  let empty = shuf_run(ctx, root, ["-r", "-e"])?
  assert empty.stderr == "shuf: no lines to repeat\n", empty.stderr

  let both = shuf_run(ctx, root, ["-e", "0", "-i", "0-2"])?
  assert "cannot be used with" in both.stderr, both.stderr

  let range = shuf_run(ctx, root, ["-i", "5-3"])?
  assert "invalid value '5-3' for '--input-range <LO-HI>': start exceeds end" in range.stderr, range.stderr

  let count = shuf_run(ctx, root, ["-n", "a"])?
  assert "invalid value 'a' for '--head-count <COUNT>': invalid digit found in string" in count.stderr, count.stderr

  let extra = shuf_run(ctx, root, ["a", "b"])?
  assert "unexpected argument 'b' found" in extra.stderr, extra.stderr

  let seed = shuf_run(ctx, root, ["--random-seed=x"])?
  assert seed.status == 1
  assert seed.stderr.starts_with("shuf: option '--random-seed' is not supported"), seed.stderr
}

test test_shuf_argument_diagnostics_match_uutils { |ctx|
  let root = test.temp_dir(ctx, name: "shuf-errors")?

  let ranges = shuf_run(ctx, root, ["-i", "2-9", "-i", "2-9"])?
  assert ranges.status == 1
  assert "--input-range" in ranges.stderr, ranges.stderr
  assert "cannot be used multiple times" in ranges.stderr, ranges.stderr

  let outputs = shuf_run(ctx, root, ["-o", "file_a", "-o", "file_b"])?
  assert outputs.status == 1
  assert "--output" in outputs.stderr, outputs.stderr
  assert "cannot be used multiple times" in outputs.stderr, outputs.stderr

  let pair = shuf_run(ctx, root, ["file_a", "file_b"])?
  assert pair.status == 1
  assert "unexpected argument 'file_b' found" in pair.stderr, pair.stderr

  let triple = shuf_run(ctx, root, ["file_a", "file_b", "file_c"])?
  assert triple.status == 1
  assert "unexpected argument 'file_b' found" in triple.stderr, triple.stderr

  let echo_range = shuf_run(ctx, root, ["-e", "0", "-i", "0-2"])?
  assert echo_range.status == 1
  assert "cannot be used with" in echo_range.stderr, echo_range.stderr

  let range_file = shuf_run(ctx, root, ["-i", "0-9", "file"])?
  assert range_file.status == 1
  assert "cannot be used with" in range_file.stderr, range_file.stderr

  let missing_dash = shuf_run(ctx, root, ["-i", "0"])?
  assert missing_dash.status == 1
  assert "invalid value '0' for '--input-range <LO-HI>': missing '-'" in missing_dash.stderr, missing_dash.stderr

  let bad_start = shuf_run(ctx, root, ["-i", "a-9"])?
  assert bad_start.status == 1
  assert "invalid value 'a-9' for '--input-range <LO-HI>': invalid digit found in string" in bad_start.stderr, bad_start.stderr

  let bad_end = shuf_run(ctx, root, ["-i", "0-b"])?
  assert bad_end.status == 1
  assert "invalid value '0-b' for '--input-range <LO-HI>': invalid digit found in string" in bad_end.stderr, bad_end.stderr

  let descending = shuf_run(ctx, root, ["-i", "5-3"])?
  assert descending.status == 1
  assert "invalid value '5-3' for '--input-range <LO-HI>': start exceeds end" in descending.stderr, descending.stderr

  let count = shuf_run(ctx, root, ["-n", "a"])?
  assert count.status == 1
  assert "invalid value 'a' for '--head-count <COUNT>': invalid digit found in string" in count.stderr, count.stderr
}

test test_shuf_large_ranges { |ctx|
  let root = test.temp_dir(ctx, name: "shuf-large-ranges")?
  let cases = [
    {args: ["-n1", "-i", "1-18446744073709551615"], lower: "1", upper: "18446744073709551615"},
    {args: ["-rn1", "-i", "1-18446744073709551615"], lower: "1", upper: "18446744073709551615"},
    {args: ["-n1", "-i", "0-18446744073709551614"], lower: "0", upper: "18446744073709551614"},
    {args: ["-rn1", "-i", "0-18446744073709551614"], lower: "0", upper: "18446744073709551614"},
  ]

  for case in cases {
    let result = shuf_run(ctx, root, case.args)?
    assert result.status == 0, result.stderr
    let records = result.stdout.utf8()?.trim().lines()
    assert records.len() == 1, records.join(",")
    assert decimal_in_range(records[0], case.lower, case.upper), records.join(",")
  }

  let singleton = shuf_run(ctx, root, ["-n1", "-i", "18446744073709551615-18446744073709551615"])?
  assert singleton.status == 0, singleton.stderr
  assert singleton.stdout == b"18446744073709551615\n"

  let uncounted_singleton = shuf_run(ctx, root, ["-i", "18446744073709551615-18446744073709551615"])?
  assert uncounted_singleton.status == 0, uncounted_singleton.stderr
  assert uncounted_singleton.stdout == b"18446744073709551615\n"

  let unique = shuf_run(ctx, root, ["-n2", "-i", "18446744073709551613-18446744073709551615"])?
  assert unique.status == 0, unique.stderr
  let selected = unique.stdout.utf8()?.trim().lines()
  assert selected.len() == 2, selected.join(",")
  assert selected[0] != selected[1], selected.join(",")
  assert decimal_in_range(selected[0], "18446744073709551613", "18446744073709551615"), selected[0]
  assert decimal_in_range(selected[1], "18446744073709551613", "18446744073709551615"), selected[1]

  fp"{root}/random".write(b"\0\0\0\0\0\0\0\0")
  let sourced = shuf_run(ctx, root, ["--random-source=random", "-n1", "-i", "1-18446744073709551615"])?
  assert sourced.status == 0, sourced.stderr
  assert sourced.stdout == b"1\n"

  let full_range = shuf_run(ctx, root, ["-n1", "-i", "0-18446744073709551615"])?
  assert full_range.status == 1
  assert "input ranges beyond 2^63 - 2 are not supported" in full_range.stderr, full_range.stderr

  let descending = shuf_run(ctx, root, ["-n1", "-i", "18446744073709551615-18446744073709551614"])?
  assert descending.status == 1
  assert "start exceeds end" in descending.stderr, descending.stderr
}

test test_shuf_invalid_utf8_operands_are_raw_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "shuf-invalid-utf8-echo")?
  let args: List[Union[Str, Path]] = ["-e", Path.parse_bytes(b"a\xFFb")?, "ok"]
  let result = shuf_raw_run(ctx, root, args)?
  assert result.status == 0, result.stderr
  assert result.stderr == ""
  assert result.stdout == b"a\xFFb\nok\n" or result.stdout == b"ok\na\xFFb\n", "both orders of the two arguments are permutations"
}

test test_shuf_reads_invalid_utf8_operand_file { |ctx|
  let root = test.temp_dir(ctx, name: "shuf-invalid-utf8-file")?
  let name = Path.parse_bytes(b"a\xFFb")?
  cd root {
    name.write("foo\n")
  }

  let args: List[Union[Str, Path]] = [name]
  let result = shuf_raw_run(ctx, root, args)?
  assert result.status == 0, result.stderr
  assert result.stderr == ""
  assert result.stdout == b"foo\n"
}
