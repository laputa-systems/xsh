type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/tsort.xsh by its real path inside `root`, capturing both streams.
proc tsort_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/tsort.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

proc tsort_run_paths(ctx: TestContext, root: Path, args: List[Path], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin, fp"{ctx.core_dir}/tsort.xsh"].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_tsort_orders_by_dependency_then_name { |ctx|
  let root = test.temp_dir(ctx, name: "tsort")?

  assert tsort_run(ctx, root, [], b"a b b c c d d e e f f g\n")?.stdout == b"a\nb\nc\nd\ne\nf\ng\n"
  assert tsort_run(ctx, root, [], b"b a\nd c\nz h x h r h\n")?.stdout == b"b\nd\nr\nx\nz\na\nc\nh\n", "ready nodes come out in name order"
  assert tsort_run(ctx, root, [], b"a b c c d e\ng g\nf g e f\nh h\n")?.stdout == b"a\nc\nd\nh\nb\ne\nf\ng\n"
  assert tsort_run(ctx, root, [], b"a b b c c d d e e f f g\nc x x y y z\n")?.stdout == b"a\nb\nc\nx\nd\ny\ne\nz\nf\ng\n"
  assert tsort_run(ctx, root, [], b"first first\nfirst second second second")?.stdout == b"first\nsecond\n", "self edges add no dependency"
  assert tsort_run(ctx, root, [], b"d d\nc c\na a\nb b")?.stdout == b"a\nb\nc\nd\n"
  assert tsort_run(ctx, root, [], b"")?.stdout == b""
}

test test_tsort_reports_and_breaks_loops { |ctx|
  let root = test.temp_dir(ctx, name: "tsort")?

  let one = tsort_run(ctx, root, [], b"a b b c c d c b")?
  assert one.status == 1
  assert one.stdout == b"a\nb\nc\nd\n"
  assert one.stderr == "tsort: -: input contains a loop:\ntsort: b\ntsort: c\n", one.stderr

  let two = tsort_run(ctx, root, [], b"a b b c c b b d d b")?
  assert two.stdout == b"a\nb\nd\nc\n"
  assert two.stderr == "tsort: -: input contains a loop:\ntsort: b\ntsort: c\ntsort: -: input contains a loop:\ntsort: b\ntsort: d\n", two.stderr

  fp"{root}/f".write("t b\nt s\ns t\n")
  let named = tsort_run(ctx, root, ["f"])?
  assert named.stdout == b"s\nt\nb\n"
  assert named.stderr == "tsort: f: input contains a loop:\ntsort: s\ntsort: t\n", named.stderr
}

test test_tsort_binary_tokens_and_errors { |ctx|
  let root = test.temp_dir(ctx, name: "tsort")?

  let binary = tsort_run(ctx, root, [], b"hrllo\nawuyues\napple\niphone\niphone\na\n\xff\n\xff\n")?
  assert binary.stdout == b"apple\nhrllo\n\xff\niphone\nawuyues\na\n", "tokens that are not UTF-8 survive and sort bytewise"

  let odd = tsort_run(ctx, root, [], b"a\n")?
  assert odd.status == 1
  assert odd.stderr == "tsort: -: input contains an odd number of tokens\n", odd.stderr

  let extra = tsort_run(ctx, root, ["f", "g"])?
  assert extra.stderr == "tsort: extra operand 'g'\nTry 'tsort --help' for more information.\n", extra.stderr

  let missing = tsort_run(ctx, root, ["nosuchfile.txt"])?
  assert missing.stderr == "tsort: nosuchfile.txt: No such file or directory\n", missing.stderr

  fp"{root}/dir".mkdir()
  let directory = tsort_run(ctx, root, ["dir"])?
  assert directory.stderr == "tsort: dir: read error: Is a directory\n", directory.stderr

  let help = tsort_run(ctx, root, ["-h"])?
  assert help.status == 0
  assert help.stdout.utf8()?.starts_with("Usage: tsort")

  let version = tsort_run(ctx, root, ["-V"])?
  assert version.status == 0
  assert version.stdout.utf8()?.starts_with("tsort")
}

test test_tsort_reads_non_utf8_input_path { |ctx|
  let root = test.temp_dir(ctx, name: "tsort-raw")?
  let input = Path.parse_bytes(bytes.concat([root.bytes(), b"/input-\xff"]))?
  input.write("a b\nb c\n")

  let result = tsort_run_paths(ctx, root, [Path.parse_bytes(input.bytes())?])?
  assert result.status == 0, result.stderr
  assert result.stdout == b"a\nb\nc\n"
}
