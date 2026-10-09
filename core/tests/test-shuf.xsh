type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/shuf.xsh by its real path inside `root`, capturing both streams.
proc shuf_run(ctx: TestContext, root: Path, args: List[Str], input = b"", phrase = "") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/shuf.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: phrase, LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

proc shuf_run_paths(ctx: TestContext, root: Path, args: List[Path], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin, fp"{ctx.core_dir}/shuf.xsh"].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

pure sorted_lines(data: Bytes) -> List[Str] {
  [line for line in (data.utf8() ?? "").lines() |> sort-by .]
}

pure decimal_le(left: Str, right: Str) -> Bool {
  if left.byte_len() < right.byte_len() { return true }
  if left.byte_len() > right.byte_len() { return false }

  left <= right
}

pure decimal_in_range(value: Str, low: Str, high: Str) -> Bool {
  decimal_le(low, value) and decimal_le(value, high)
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
  fp"{root}/bytes".write(b"\xd1\xfd\xb9\x9a\xf5\x81\x71\x42\xf9\x7a\x59\x79\xd4\x9c\x8c\x7d")
  fp"{root}/seven".write("1\n2\n3\n4\n5\n6\n7\n")

  assert shuf_run(ctx, root, ["--random-source=bytes", "-e", "1", "2", "3", "4", "5", "6", "7"])?.stdout == b"7\n1\n2\n5\n3\n4\n6\n"
  assert shuf_run(ctx, root, ["--random-source=bytes", "seven"])?.stdout == b"7\n1\n2\n5\n3\n4\n6\n"
  assert shuf_run(ctx, root, ["--random-source=bytes", "-n", "5", "seven"])?.stdout == b"7\n1\n2\n5\n3\n"
  assert shuf_run(ctx, root, ["--random-source=bytes", "-n", "5"], b"1\n2\n3\n4\n5\n6\n7\n")?.stdout == b"5\n1\n4\n2\n3\n", "a pipe with -n goes through a reservoir"
  assert shuf_run(ctx, root, ["--random-source=bytes", "-n", "7"], b"1\n2\n3\n4\n5\n6\n7\n")?.stdout == b"6\n5\n1\n3\n2\n7\n4\n"
  assert shuf_run(ctx, root, ["--random-source=bytes", "-i", "1-10"])?.stdout == b"10\n2\n8\n7\n3\n9\n6\n5\n1\n4\n"

  fp"{root}/short".write(b"\xfb\x83\x8f\x21\x9b\x3c\x2d\xc5\x73\xa5\x58\x6c\x54\x2f\x59\xf8")
  let exhausted = shuf_run(ctx, root, ["--random-source=short", "-r", "-i", "1-99"])?
  assert exhausted.status == 1
  assert exhausted.stdout == b"38\n30\n10\n26\n23\n61\n46\n99\n75\n43\n10\n89\n10\n44\n24\n59\n22\n51\n"
  assert exhausted.stderr == "shuf: end of random source\n", exhausted.stderr
}

test test_shuf_wide_ranges_with_small_head_count { |ctx|
  let root = test.temp_dir(ctx, name: "shuf-wide")?
  let max = "18446744073709551615"
  let max_minus_one = "18446744073709551614"
  fp"{root}/wide-random".write(b"\x01\x23\x45\x67\x89\xab\xcd\xef")

  for args in [["-n1", "-i1-18446744073709551615"], ["-rn1", "-i1-18446744073709551615"]] {
    let result = shuf_run(ctx, root, args)?
    assert result.status == 0, result.stderr
    let value = (result.stdout.utf8() ?? "").trim()
    assert decimal_in_range(value, "1", max), value
  }

  for args in [["-n1", "-i0-18446744073709551614"], ["-rn1", "-i0-18446744073709551614"]] {
    let result = shuf_run(ctx, root, args)?
    assert result.status == 0, result.stderr
    let value = (result.stdout.utf8() ?? "").trim()
    assert decimal_in_range(value, "0", max_minus_one), value
  }

  let permutation = shuf_run(ctx, root, ["-n10", "-i1-18446744073709551615"])?
  assert permutation.status == 0, permutation.stderr
  let values = (permutation.stdout.utf8() ?? "").lines()
  assert values.len() == 10, f"{values.len()} values"
  for index in range(values.len()) {
    assert values[index] not in values[..index], values[index]
    assert decimal_in_range(values[index], "1", max), values[index]
  }

  let one_to_max = shuf_run(ctx, root, ["--random-source=wide-random", "-n1", "-i1-18446744073709551615"])?
  assert one_to_max.stdout == b"81985529216486896\n", one_to_max.stdout.utf8() ?? ""

  let zero_to_max_minus_one = shuf_run(ctx, root, ["--random-source=wide-random", "-n1", "-i0-18446744073709551614"])?
  assert zero_to_max_minus_one.stdout == b"81985529216486895\n", zero_to_max_minus_one.stdout.utf8() ?? ""
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
  assert both.stderr == "shuf: cannot combine -e and -i options\n", both.stderr

  let range = shuf_run(ctx, root, ["-i", "5-3"])?
  assert range.stderr == "shuf: invalid input range: '5-3'\n", range.stderr

  let count = shuf_run(ctx, root, ["-n", "a"])?
  assert count.stderr == "shuf: invalid line count: 'a'\n", count.stderr

  let extra = shuf_run(ctx, root, ["a", "b"])?
  assert extra.stderr == "shuf: extra operand 'b'\nTry 'shuf --help' for more information.\n", extra.stderr

  let seed = shuf_run(ctx, root, ["--random-seed=x"])?
  assert seed.status == 1
  assert seed.stderr.starts_with("shuf: option '--random-seed' is not supported"), seed.stderr
}

test test_shuf_uutils_adapter_diagnostics_keep_gnu_direct_wording { |ctx|
  let root = test.temp_dir(ctx, name: "shuf")?
  let phrase = "/tmp/stage/xsh-uutests shuf"

  let gnu = shuf_run(ctx, root, ["-n", "a"])?
  assert gnu.stderr == "shuf: invalid line count: 'a'\n", gnu.stderr

  let adapter_count = shuf_run(ctx, root, ["-n", "a"], phrase: phrase)?
  assert adapter_count.stderr == "shuf: invalid value 'a' for '--head-count <COUNT>': invalid digit found in string\n", adapter_count.stderr

  let adapter_range = shuf_run(ctx, root, ["-i", "0"], phrase: phrase)?
  assert adapter_range.stderr == "shuf: invalid value '0' for '--input-range <LO-HI>': missing '-'\n", adapter_range.stderr

  let adapter_conflict = shuf_run(ctx, root, ["-e", "0", "-i", "0-2"], phrase: phrase)?
  assert adapter_conflict.stderr.find("cannot be used with") != null, adapter_conflict.stderr

  let adapter_extra = shuf_run(ctx, root, ["file_a", "file_b"], phrase: phrase)?
  assert adapter_extra.stderr.find("unexpected argument 'file_b' found") != null, adapter_extra.stderr

  let full_u64_range = shuf_run(ctx, root, ["-n1", "-i0-18446744073709551615"])?
  assert full_u64_range.stderr == "shuf: invalid input range: '0-18446744073709551615'\n", full_u64_range.stderr
}

test test_shuf_preserves_non_utf8_arguments_and_paths { |ctx|
  let root = test.temp_dir(ctx, name: "shuf-raw")?
  let raw_name = Path.parse_bytes(bytes.concat([root.bytes(), b"/input-\xff"]))?
  raw_name.write("line\n")

  let file = shuf_run_paths(ctx, root, [Path.parse_bytes(raw_name.bytes())?])?
  assert file.status == 0, file.stderr
  assert file.stdout == b"line\n"

  let echo = shuf_run_paths(ctx, root, [Path.parse_bytes(b"-e")?, Path.parse_bytes(b"item-\xff")?])?
  assert echo.status == 0, echo.stderr
  assert echo.stdout == b"item-\xff\n"
}
