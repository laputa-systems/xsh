type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/csplit.xsh by its real path inside `root`, capturing both streams.
proc csplit_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/csplit.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

# Lines FROM up to but not including TO, one number per line.
pure numbers(from: Int, to: Int) -> Bytes {
  bytes.concat([bytes.from_text(f"{n}\n") for n in range(from, to)])
}

proc piece(root: Path, name: Str) [fs, error] -> Result[Bytes] {
  fp"{root}/{name}".read_bytes()?
}

proc clean(root: Path) [fs, error] {
  for entry in fs.children(root)? {
    if entry.name.starts_with("xx") or entry.name.starts_with("dog") {
      entry.path.remove()
    }
  }
}

test test_csplit_line_number_patterns { |ctx|
  let root = test.temp_dir(ctx, name: "csplit")?
  fp"{root}/n".write(numbers(1, 51))

  assert csplit_run(ctx, root, ["n", "10"])?.stdout == b"18\n123\n"
  assert piece(root, "xx00")? == numbers(1, 10)
  assert piece(root, "xx01")? == numbers(10, 51)
  clean(root)

  assert csplit_run(ctx, root, ["n", "10", "{2}"])?.stdout == b"18\n30\n30\n63\n"
  assert piece(root, "xx02")? == numbers(20, 30)
  clean(root)

  assert csplit_run(ctx, root, ["n", "/20/", "10", "/40/"])?.stdout == b"48\n0\n60\n33\n", "an already passed line number gives an empty piece"
  clean(root)

  assert csplit_run(ctx, root, ["-", "2", "4"], b"a\nb\nc\nd")?.stdout == b"2\n4\n1\n", "a final unterminated line is counted"
}

test test_csplit_regex_patterns_and_offsets { |ctx|
  let root = test.temp_dir(ctx, name: "csplit")?
  fp"{root}/n".write(numbers(1, 51))

  assert csplit_run(ctx, root, ["n", "/9$/", "{*}"])?.stdout == b"16\n29\n30\n30\n30\n6\n"
  let made = fs.children(root)? |> where { |e| e.name.starts_with("xx") } |> count()
  assert made == 6
  clean(root)

  assert csplit_run(ctx, root, ["n", "/9$/+3"])?.stdout == b"24\n117\n"
  assert piece(root, "xx00")? == numbers(1, 12)
  clean(root)

  assert csplit_run(ctx, root, ["n", "/9$/-3"])?.stdout == b"10\n131\n"
  assert piece(root, "xx01")? == numbers(6, 51)
  clean(root)

  assert csplit_run(ctx, root, ["n", "%23%"])?.stdout == b"84\n", "%REGEXP% skips without writing a piece"
  assert piece(root, "xx00")? == numbers(23, 51)
  clean(root)

  assert csplit_run(ctx, root, ["n", "%0$%", "/^4/"])?.stdout == b"90\n33\n"
  clean(root)

  assert csplit_run(ctx, root, ["--suppress-matched", "n", "/10$/", "/12$/-1"])?.stdout == b"18\n0\n117\n"
  assert piece(root, "xx02")? == numbers(12, 51)
}

test test_csplit_errors_remove_files_unless_kept { |ctx|
  let root = test.temp_dir(ctx, name: "csplit")?
  fp"{root}/n".write(numbers(1, 51))

  let gone = csplit_run(ctx, root, ["n", "/20/", "/nope/"])?
  assert gone.status == 1
  assert gone.stdout == b"48\n93\n"
  assert gone.stderr == "csplit: '/nope/': match not found\n", gone.stderr
  assert ! fp"{root}/xx00".exists()?

  let kept = csplit_run(ctx, root, ["-k", "n", "/20/", "/nope/"])?
  assert kept.status == 1
  assert piece(root, "xx00")? == numbers(1, 20)
  assert piece(root, "xx01")? == numbers(20, 51)
  clean(root)

  let range = csplit_run(ctx, root, ["n", "40", "{2}"])?
  assert range.stdout == b"108\n33\n"
  assert range.stderr == "csplit: '40': line number out of range on repetition 1\n", range.stderr

  let repeat = csplit_run(ctx, root, ["n", "/9$/", "{50}"])?
  assert repeat.stderr == "csplit: '/9$/': match not found on repetition 5\n", repeat.stderr

  let negative = csplit_run(ctx, root, ["n", "/3$/-3"])?
  assert negative.stdout == b"0\n"
  assert negative.stderr == "csplit: '/3$/-3': line number out of range\n", negative.stderr
}

test test_csplit_options { |ctx|
  let root = test.temp_dir(ctx, name: "csplit")?
  fp"{root}/n".write(numbers(1, 51))

  assert csplit_run(ctx, root, ["-q", "n", "13", "%25%", "/0$/"])?.stdout == b""
  assert piece(root, "xx01")? == numbers(25, 30)
  clean(root)

  assert csplit_run(ctx, root, ["--prefix", "dog", "n", "10"])?.status == 0
  assert piece(root, "dog01")? == numbers(10, 51)
  clean(root)

  assert csplit_run(ctx, root, ["-n", "3", "n", "10"])?.status == 0
  assert piece(root, "xx001")? == numbers(10, 51)
  clean(root)

  assert csplit_run(ctx, root, ["--suffix-format", "%#6.3x", "n", "10"])?.status == 0
  assert piece(root, "xx 0x001")? == numbers(10, 51)
  clean(root)

  assert csplit_run(ctx, root, ["--suffix-format", "-%02d", "n", "10"])?.status == 0
  assert piece(root, "xx-01")? == numbers(10, 51), "a format starting with a hyphen is a value"
  clean(root)

  assert csplit_run(ctx, root, ["-z", "--suppress-matched", "n", "/0$/", "{*}"])?.stdout == b"18\n27\n27\n27\n27\n"
  assert ! fp"{root}/xx05".exists()?, "-z elides the empty last piece"
}

test test_csplit_operand_and_format_errors { |ctx|
  let root = test.temp_dir(ctx, name: "csplit")?
  fp"{root}/n".write(numbers(1, 5))

  let none = csplit_run(ctx, root, [])?
  assert none.stderr == "csplit: missing operand\nTry 'csplit --help' for more information.\n", none.stderr

  let nopattern = csplit_run(ctx, root, ["n"])?
  assert nopattern.stderr == "csplit: missing operand after 'n'\nTry 'csplit --help' for more information.\n", nopattern.stderr

  let zero = csplit_run(ctx, root, ["n", "0"])?
  assert zero.stderr == "csplit: 0: line number must be greater than zero\n", zero.stderr

  let order = csplit_run(ctx, root, ["n", "3", "2"])?
  assert order.stderr == "csplit: line number '2' is smaller than preceding line number, 3\n", order.stderr

  let pattern = csplit_run(ctx, root, ["n", "bad"])?
  assert pattern.stderr == "csplit: 'bad': invalid pattern\n", pattern.stderr

  let format = csplit_run(ctx, root, ["-b", "no conversion", "n", "2"])?
  assert format.stderr == "csplit: missing conversion specifier in suffix\n", format.stderr

  let many = csplit_run(ctx, root, ["-b", "%d%d", "n", "2"])?
  assert many.stderr == "csplit: too many % conversion specifications in suffix\n", many.stderr

  let missing = csplit_run(ctx, root, ["in", "1"])?
  assert missing.stderr == "csplit: cannot open 'in' for reading: No such file or directory\n", missing.stderr
}

test test_csplit_stdout_write_error { |ctx|
  if ! p"/dev/full".exists()? { test.skip("requires /dev/full"); return }

  let root = test.temp_dir(ctx, name: "csplit")?
  fp"{root}/n".write(numbers(1, 5))
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/csplit.xsh".display(), fp"{root}/n".display(), "1"]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", p"/dev/full", err)
  let status = process.run(plan)?

  assert status.exit_code()? == 1
  assert err.read_text()? == "csplit: No space left on device\n", err.read_text()?
}

test test_csplit_matches_gnu_current_line_rules { |ctx|
  let root = test.temp_dir(ctx, name: "csplit")?
  fp"{root}/n".write(numbers(1, 21))

  let after_line = csplit_run(ctx, root, ["n", "3", "/3/"])?
  assert after_line.stdout == b"4\n0\n47\n", "a line number leaves its own line to be matched"
  clean(root)

  let after_offset = csplit_run(ctx, root, ["n", "/^5$/+1", "/^6$/"])?
  assert after_offset.status == 1
  assert after_offset.stdout == b"10\n41\n"
  assert after_offset.stderr == "csplit: '/^6$/': match not found\n", "a regexp match leaves the line after its offset unmatched"

  let last = csplit_run(ctx, root, ["n", "/20/", "5"])?
  assert last.stdout == b"48\n0\n"
  assert last.stderr == "csplit: '5': line number out of range\n", last.stderr

  let gone = csplit_run(ctx, root, ["n", "/20/+1", "5"])?
  assert gone.status == 1
  assert gone.stdout == b"51\n"
  assert gone.stderr == "csplit: input disappeared\n", gone.stderr
  assert piece(root, "xx00")? == numbers(1, 21), "finished pieces stay"
  assert piece(root, "xx01")? == b"", "and so does the piece being opened"
  clean(root)

  let behind = csplit_run(ctx, root, ["n", "/^15$/-3", "14", "/^15$/"])?
  assert behind.stdout == b"24\n6\n21\n", "a line number leaves the lines already read by a negative offset unmatched"
  assert behind.stderr == "csplit: '/^15$/': match not found\n", behind.stderr

  let over = csplit_run(ctx, root, ["--suppress-matched", "n", "21", "30"])?
  assert over.stdout == b"51\n0\n"
  assert over.stderr == "csplit: '30': line number out of range\n", over.stderr

  let spare = csplit_run(ctx, root, ["--suppress-matched", "n", "21"])?
  assert spare.status == 0, "a line number one past the end is not out of range when the matched line is suppressed"
  assert spare.stdout == b"51\n0\n"
}
