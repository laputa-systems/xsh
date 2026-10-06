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
