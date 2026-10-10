test test_xargs_quotes_and_nul_preserve_argument_boundaries { |ctx|
  let root = test.temp_dir(ctx, name: "xargs-args")?
  let input = fp"{root}/input"
  input.write("one 'two words' three\\ four\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- printf "<%s>\n" < $input
  assert out == "<one>\n<two words>\n<three four>\n"
  input.write(b"one two\0three\xff\0")
  let nul = run.bytes ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- -0 printf "<%s>\n" < $input
  assert nul == b"<one two>\n<three\xff>\n"
}

test test_xargs_batch_replacement_empty_and_status { |ctx|
  let root = test.temp_dir(ctx, name: "xargs-batch")?
  let input = fp"{root}/input"
  input.write("one\ntwo\nthree\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- -n2 printf "[%s][%s]\n" < $input
  assert out == "[one][two]\n[three][]\n"
  let replaced = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- "-I{}" printf "<{}>\n" < $input
  assert replaced == "<one>\n<two>\n<three>\n"
  input.write("")
  let empty = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- -r printf "called" < $input
  assert empty == ""
  input.write("arg\n")
  let failure = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- sh -c "exit 7" < $input
  assert failure.status.exited_with(123)
}

test test_xargs_eof_marker_flushes_preceding_batch { |ctx|
  let root = test.temp_dir(ctx, name: "xargs-marker")?
  let input = fp"{root}/input"
  input.write("one\ntwo\nSTOP\nignored\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- -E STOP printf "<%s>\n" < $input
  assert out == "<one>\n<two>\n"
}

test test_xargs_parallel_and_fatal_child_status { |ctx|
  let root = test.temp_dir(ctx, name: "xargs-parallel")?
  let input = fp"{root}/input"
  input.write("one\ntwo\nthree\n")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- -P2 -n1 sh -c "printf '%s\\n' \"$1\"" sh < $input
  assert result.status.exited_with(0)
  assert result.stdout.split("\n").len() == 4
  let fatal = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- -n1 sh -c "exit 255" < $input
  assert fatal.status.exited_with(124)
}

test test_xargs_logical_lines_empty_replacement_and_size_bound { |ctx|
  let root = test.temp_dir(ctx, name: "xargs-lines")?
  let input = fp"{root}/input"
  input.write("one two\n\nthree\nfour\n")
  let lines = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- -L2 sh -c "printf '%s\\n' \"$#\"" sh < $input
  assert lines == "3\n1\n"
  input.write("")
  let empty = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- "-I{}" printf called < $input
  assert empty == ""
  input.write("long-argument-value\n")
  let bounded = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- -s20 printf "%s" < $input
  assert bounded.status.exited_with(1)
  assert bounded.stdout == ""
}

test test_xargs_argfile_preserves_child_standard_input { |ctx|
  let root = test.temp_dir(ctx, name: "xargs-argfile")?
  let input = fp"{root}/input"
  let arguments = fp"{root}/args"
  input.write("payload\n")
  arguments.write("unused\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- -a $arguments sh -c "cat" < $input
  assert out == "payload\n"
}

test test_xargs_token_spans_reader_chunk_boundary { |ctx|
  let root = test.temp_dir(ctx, name: "xargs-chunk")?
  let input = fp"{root}/input"
  var block = "v"
  for index in range(16) { block = f"{block}{block}" }
  let prefix = block.byte_slice(0,65535)
  input.write(f"{prefix}\\ tail\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- printf "%s" < $input
  assert out.byte_len() == 65540
  assert out.byte_slice(65535) == " tail"
}

test test_xargs_trailing_blanks_continue_logical_line { |ctx|
  let root = test.temp_dir(ctx, name: "xargs-continuation")?
  let input = fp"{root}/input"
  input.write("one \ntwo\nthree\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/xargs.xsh" -- -L1 sh -c "printf '%s\\n' \"$#\"" sh < $input
  assert out == "2\n1\n"
}

use core.lib.findutils_fixture as fixture

# Every command line of the recorded GNU xargs corpus, in four shares that run
# in parallel; the corpus and the fixture tree are described in
# tests/data/findutils/regen.xsh.
proc check_share(ctx: TestContext, part: Int) [fs, process, time, error] -> Result[Unit, Error] {
  let root = test.temp_dir(ctx, name: f"xargs-corpus-{part}")?
  let work = test.temp_dir(ctx, name: f"xargs-corpus-out-{part}")?
  fixture.build(root)?
  fixture.build_hostile(root)?
  defer fixture.restore(root)
  let corpus = fp"{ctx.core_dir}/tests/data/findutils/xargs.jsonl"
  let failures = fixture.run_corpus(ctx.xsh_bin, fp"{ctx.core_dir}/xargs.xsh", corpus, root, work, part, 4)?
  assert failures.is_empty(), failures.join("\n")
  Ok()
}

test test_xargs_matches_recorded_gnu_output_0 { |ctx| check_share(ctx, 0)? }
test test_xargs_matches_recorded_gnu_output_1 { |ctx| check_share(ctx, 1)? }
test test_xargs_matches_recorded_gnu_output_2 { |ctx| check_share(ctx, 2)? }
test test_xargs_matches_recorded_gnu_output_3 { |ctx| check_share(ctx, 3)? }

# A command word and an -a file name that are not valid UTF-8 reach the
# command unchanged.
test test_xargs_accepts_non_utf8_command_and_file { |ctx|
  let root = test.temp_dir(ctx, name: "xargs-raw-operands")?
  let list = Path.parse_bytes(bytes.concat([root.bytes(), b"/l\xff"]))?
  list.write("one two\n")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/xargs.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin, p"--", script, p"-a", list, p"printf", p"[%s]", Path.parse_bytes(b"\xff:")?], root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  assert status.exited_with(0), err.read_text()?
  assert out.read_bytes()? == b"[\xff:][one][two]"
}
