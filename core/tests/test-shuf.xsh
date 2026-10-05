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

pure sorted_lines(data: Bytes) -> List[Str] {
  [line for line in (data.utf8() ?? "").lines() |> sort-by .]
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
  assert shuf_run(ctx, root, ["--random-source=bytes", "-i", "1-10"])?.stdout == b"10\n2\n8\n7\n3\n9\n6\n5\n1\n4\n"

  fp"{root}/short".write(b"\xfb\x83\x8f\x21\x9b\x3c\x2d\xc5\x73\xa5\x58\x6c\x54\x2f\x59\xf8")
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
