type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/comm.xsh by its real path inside `root`, capturing both streams.
proc comm_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/comm.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_comm_three_columns_and_suppression { |ctx|
  let root = test.temp_dir(ctx, name: "comm")?
  fp"{root}/a".write("a\nz\n")
  fp"{root}/b".write("b\nz\n")

  assert comm_run(ctx, root, ["a", "b"])?.stdout == b"a\n\tb\n\t\tz\n"
  assert comm_run(ctx, root, ["a", "b", "-1"])?.stdout == b"b\n\tz\n"
  assert comm_run(ctx, root, ["a", "b", "-2"])?.stdout == b"a\n\tz\n"
  assert comm_run(ctx, root, ["a", "b", "-3"])?.stdout == b"a\n\tb\n"
  assert comm_run(ctx, root, ["-12", "a", "b"])?.stdout == b"z\n"
  assert comm_run(ctx, root, ["-123123", "a", "b"])?.stdout == b"", "options bundle and repeat"
}

test test_comm_output_delimiter_total_and_zero { |ctx|
  let root = test.temp_dir(ctx, name: "comm")?
  fp"{root}/a".write("a\nz\n")
  fp"{root}/b".write("b\nz\n")
  fp"{root}/an".write(b"a\0z\0")
  fp"{root}/bn".write(b"b\0z\0")

  assert comm_run(ctx, root, ["--output-delimiter=word", "a", "b"])?.stdout == b"a\nwordb\nwordwordz\n"
  assert comm_run(ctx, root, ["--output-delimiter", "-1", "a", "b"])?.stdout == b"a\n-1b\n-1-1z\n", "the delimiter may start with a hyphen"
  assert comm_run(ctx, root, ["--output-delimiter=", "a", "b"])?.stdout == b"a\n\0b\n\0\0z\n", "an empty delimiter is NUL"
  assert comm_run(ctx, root, ["--total", "a", "b"])?.stdout == b"a\n\tb\n\t\tz\n1\t1\t1\ttotal\n"
  assert comm_run(ctx, root, ["--total", "-123", "a", "b"])?.stdout == b"1\t1\t1\ttotal\n"
  assert comm_run(ctx, root, ["-z", "--total", "an", "bn"])?.stdout == b"a\0\tb\0\t\tz\x001\t1\t1\ttotal\0"

  let clash = comm_run(ctx, root, ["--output-delimiter=x", "--output-delimiter=y", "a", "b"])?
  assert clash.status == 1
  assert clash.stderr == "comm: multiple output delimiters specified\n", clash.stderr
  assert comm_run(ctx, root, ["--output-delimiter=x", "--output-delimiter=x", "a", "b"])?.status == 0
}

test test_comm_order_checks { |ctx|
  let root = test.temp_dir(ctx, name: "comm")?
  fp"{root}/bad1".write("e\nd\nb\na\n")
  fp"{root}/bad2".write("e\nc\nb\na\n")
  fp"{root}/a".write("a\n")
  fp"{root}/c1".write("1\n3")
  fp"{root}/c2".write("3\n2")

  let checked = comm_run(ctx, root, ["--check-order", "bad1", "bad2"])?
  assert checked.status == 1
  assert checked.stdout == b"\t\te\n"
  assert checked.stderr == "comm: file 2 is not in sorted order\n", checked.stderr

  let unchecked = comm_run(ctx, root, ["--nocheck-order", "bad1", "bad2"])?
  assert unchecked.status == 0
  assert unchecked.stdout == b"\t\te\n\tc\n\tb\n\ta\nd\nb\na\n"
  assert unchecked.stderr == ""

  let default = comm_run(ctx, root, ["a", "bad1"])?
  assert default.status == 1
  assert default.stdout == b"a\n\te\n\td\n\tb\n\ta\n"
  assert default.stderr == "comm: file 2 is not in sorted order\ncomm: input is not in sorted order\n", default.stderr

  let sorted = comm_run(ctx, root, ["c1", "c2"])?
  assert sorted.stdout == b"1\n\t\t3\n\t2\n"
  assert sorted.stderr == "comm: file 2 is not in sorted order\ncomm: input is not in sorted order\n", sorted.stderr

  let same = comm_run(ctx, root, ["bad1", "bad1"])?
  assert same.status == 0, "identical unsorted files still pair line by line"
  assert same.stdout == b"\t\te\n\t\td\n\t\tb\n\t\ta\n"
}

test test_comm_operand_and_file_errors { |ctx|
  let root = test.temp_dir(ctx, name: "comm")?
  fp"{root}/dir".mkdir()

  let none = comm_run(ctx, root, [])?
  assert none.status == 1
  assert none.stderr == "comm: missing operand\nTry 'comm --help' for more information.\n", none.stderr

  let one = comm_run(ctx, root, ["a"])?
  assert one.stderr == "comm: missing operand after 'a'\nTry 'comm --help' for more information.\n", one.stderr

  let extra = comm_run(ctx, root, ["a", "b", "c"])?
  assert extra.stderr == "comm: extra operand 'c'\nTry 'comm --help' for more information.\n", extra.stderr

  let missing = comm_run(ctx, root, ["nosuch1", "nosuch2"])?
  assert missing.status == 1
  assert missing.stderr == "comm: nosuch1: No such file or directory\n", missing.stderr

  let directory = comm_run(ctx, root, ["dir", "dir"])?
  assert directory.status == 1
  assert directory.stderr == "comm: dir: Is a directory\n", directory.stderr
}

test test_comm_reads_standard_input_and_help { |ctx|
  let root = test.temp_dir(ctx, name: "comm")?
  fp"{root}/b".write("b\nz\n")

  assert comm_run(ctx, root, ["-", "b"], b"a\nz\n")?.stdout == b"a\n\tb\n\t\tz\n"

  let help = comm_run(ctx, root, ["--help"])?
  assert help.status == 0
  assert help.stdout.utf8()?.starts_with("Usage: comm [OPTION]... FILE1 FILE2")
}
