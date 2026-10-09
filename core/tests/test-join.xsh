type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/join.xsh by its real path inside `root`, capturing both streams.
proc join_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/join.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

proc join_run_paths(ctx: TestContext, root: Path, args: List[Path], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin, fp"{ctx.core_dir}/join.xsh"].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

proc join_fixtures(root: Path) [fs, error] -> Result[Unit] {
  fp"{root}/f1".write("1\n2\n3\n5\n8\n")
  fp"{root}/f2".write("1 a\n2 b\n3 c\n4 d\n5 e\n6 f\n7 g\n8 h\n9 i\n")
  fp"{root}/f3".write("a 2 f\nb 3 g\nc 4 h\nf 5 i\ng 6 j\nh 7 k\ni 99 l\n")
}

test test_join_pairs_on_the_first_field { |ctx|
  let root = test.temp_dir(ctx, name: "join")?
  join_fixtures(root)?

  assert join_run(ctx, root, ["f1", "f2"])?.stdout == b"1 a\n2 b\n3 c\n5 e\n8 h\n"
  assert join_run(ctx, root, ["-1", "2", "f3", "f2"])?.stdout == b"2 a f b\n3 b g c\n4 c h d\n5 f i e\n6 g j f\n7 h k g\n"
  assert join_run(ctx, root, ["-j", "2", "f3", "f3"])?.stdout.utf8()?.starts_with("2 a f a f\n")
  fp"{root}/c1".write("A 1\nb 2\n")
  fp"{root}/c2".write("a x\nB y\n")
  assert join_run(ctx, root, ["-i", "c1", "c2"])?.stdout == b"A 1 x\nb 2 y\n", "-i compares without regard to case"
  assert join_run(ctx, root, ["c1", "c2"])?.stdout == b"", "and is exact without it"
}

test test_join_unpairable_lines { |ctx|
  let root = test.temp_dir(ctx, name: "join")?
  join_fixtures(root)?

  assert join_run(ctx, root, ["-a", "2", "f1", "f2"])?.stdout == b"1 a\n2 b\n3 c\n4 d\n5 e\n6 f\n7 g\n8 h\n9 i\n"
  assert join_run(ctx, root, ["-v", "2", "f1", "f2"])?.stdout == b"4 d\n6 f\n7 g\n9 i\n", "-v prints only the unpairable lines"
  assert join_run(ctx, root, ["-a", "1", "-v", "2", "f1", "f2"])?.stdout == b"4 d\n6 f\n7 g\n9 i\n"
}

test test_join_output_format_and_empty_filler { |ctx|
  let root = test.temp_dir(ctx, name: "join")?
  join_fixtures(root)?

  assert join_run(ctx, root, ["-o", "2.2 1.1", "f1", "f2"])?.stdout == b"a 1\nb 2\nc 3\ne 5\nh 8\n"
  assert join_run(ctx, root, ["-o", "2.2", "-o", "1.1", "f1", "f2"])?.stdout == b"a 1\nb 2\nc 3\ne 5\nh 8\n", "-o accumulates"
  assert join_run(ctx, root, ["-o", "0,2.2", "f1", "f2"])?.stdout == b"1 a\n2 b\n3 c\n5 e\n8 h\n"
  assert join_run(ctx, root, ["-a", "2", "-e", "x", "-o", "0,1.2,2.2", "f1", "f2"])?.stdout == b"1 x a\n2 x b\n3 x c\n4 x d\n5 x e\n6 x f\n7 x g\n8 x h\n9 x i\n"

  let auto = join_run(ctx, root, ["-o", "auto", "f2", "f1"])?
  assert auto.stdout == b"1 a\n2 b\n3 c\n5 e\n8 h\n", auto.stdout.utf8()?
}

test test_join_field_separators { |ctx|
  let root = test.temp_dir(ctx, name: "join")?
  fp"{root}/s1".write("1;a\n2;b\n")
  fp"{root}/s2".write("2;x\n3;y\n")
  fp"{root}/gap".write("a,,b\n")
  fp"{root}/sp".write(" a  ,,,b\n")

  assert join_run(ctx, root, ["-t", ";", "s1", "s2"])?.stdout == b"2;b;x\n"
  assert join_run(ctx, root, ["-t", ",", "gap", "gap"])?.stdout == b"a,,b,,b\n"
  assert join_run(ctx, root, ["-t", ",", "-e", "EMPTY", "gap", "gap"])?.stdout == b"a,EMPTY,b,EMPTY,b\n"
  assert join_run(ctx, root, ["sp", "-"], b" a  ,c ")?.stdout == b"a ,,,b ,c \n", "blank runs separate fields"
  fp"{root}/lines".write("1 a\n8 h\n")
  assert join_run(ctx, root, ["-t", "", "lines", "lines"])?.stdout == b"1 a\n8 h\n", "an empty separator joins on the whole line"

  let multi = join_run(ctx, root, ["-t", "ab", "s1", "s2"])?
  assert multi.status == 1
  assert multi.stderr == "join: multi-character tab 'ab'\n", multi.stderr
}

test test_join_order_checks { |ctx|
  let root = test.temp_dir(ctx, name: "join")?
  fp"{root}/f2".write("1 a\n2 b\n3 c\n4 d\n5 e\n6 f\n7 g\n8 h\n9 i\n")
  fp"{root}/f4".write("2 c 1 cd\n3 d 2 de\n5 e 3 ef\n7 f 4 fg\n11 g 5 gh\n")

  let warned = join_run(ctx, root, ["f2", "f4"])?
  assert warned.status == 1
  assert warned.stdout.utf8()?.find("7 g f 4 fg") != null
  assert warned.stderr == "join: f4:5: is not sorted: 11 g 5 gh\njoin: input is not in sorted order\n", warned.stderr

  let fatal = join_run(ctx, root, ["--check-order", "f2", "f4"])?
  assert fatal.status == 1
  assert fatal.stdout.utf8()?.find("7 g f 4 fg") == null
  assert fatal.stderr == "join: f4:5: is not sorted: 11 g 5 gh\n", fatal.stderr

  assert join_run(ctx, root, ["--nocheck-order", "f2", "f4"])?.stderr == ""
}

test test_join_headers_and_errors { |ctx|
  let root = test.temp_dir(ctx, name: "join")?
  fp"{root}/h1".write("id field\n1 a\n2 b\n")
  fp"{root}/h2".write("id count\n1 10\n2 25\n")

  assert join_run(ctx, root, ["--header", "h1", "h2"])?.stdout == b"id field count\n1 a 10\n2 b 25\n"

  let both = join_run(ctx, root, ["-", "-"])?
  assert both.stderr == "join: both files cannot be standard input\n", both.stderr

  let fields = join_run(ctx, root, ["-j", "3", "-1", "5", "h1", "h2"])?
  assert fields.stderr == "join: incompatible join fields 3, 5\n", fields.stderr

  let zero = join_run(ctx, root, ["-j", "0", "h1", "h2"])?
  assert zero.stderr == "join: invalid field number: '0'\n", zero.stderr

  let format = join_run(ctx, root, ["-o", "", "h1", "h2"])?
  assert format.stderr == "join: invalid file number in field spec: ''\n", format.stderr

  let missing = join_run(ctx, root, ["nosuch", "h2"])?
  assert missing.stderr == "join: nosuch: No such file or directory\n", missing.stderr
}

test test_join_preserves_non_utf8_separator_and_paths { |ctx|
  let root = test.temp_dir(ctx, name: "join-raw")?
  let one = Path.parse_bytes(bytes.concat([root.bytes(), b"/one-\xff"]))?
  let two = fp"{root}/two"
  one.write(b"a\xa7b\n")
  two.write(b"a\xa7b\n")

  let result = join_run_paths(ctx, root, [
    Path.parse_bytes(b"-t")?,
    Path.parse_bytes(b"\xa7")?,
    Path.parse_bytes(one.bytes())?,
    Path.parse_bytes(two.bytes())?,
  ])?
  assert result.status == 0, result.stderr
  assert result.stdout == b"a\xa7b\xa7b\n"
}
