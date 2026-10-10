test test_grep_basic_prefix_count_and_exit_status { |ctx|
  let root = test.temp_dir(ctx, name: "grep-basic")?
  let file = fp"{root}/input"
  file.write("red\nblue\nred blue\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -n red $file
  assert out == "1:red\n3:red blue\n"
  let observed1 = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -vc red $file
  assert observed1 == "1\n"
  let absent = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- missing $file
  assert absent.status.exited_with(1)
}

test test_grep_bre_backreference_and_ere_alternation { |ctx|
  let root = test.temp_dir(ctx, name: "grep-regex")?
  let file = fp"{root}/input"
  file.write("abab\nabac\nred\nblue\n")
  let observed2 = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- "\\(ab\\)\\1" $file
  assert observed2 == "abab\n"
  let observed3 = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -E "^(red|blue)$" $file
  assert observed3 == "red\nblue\n"
}

test test_grep_fixed_binary_and_nul_records { |ctx|
  let root = test.temp_dir(ctx, name: "grep-bytes")?
  let file = fp"{root}/input"
  file.write(b"a.b\xff\naxb\n")
  let out = run.bytes ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -Fa a.b $file
  assert out == b"a.b\xff\n"
  file.write(b"one\0two\0")
  let observed4 = run.bytes ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -Fz two $file
  assert observed4 == b"two\0"
}

test test_grep_recursive_word_and_context { |ctx|
  let root = test.temp_dir(ctx, name: "grep-recursive")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  fp"{dir}/file".write("before\nred\nafter\nredder\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -hw -A1 red fp"{dir}/file"
  assert out == "red\nafter\n"
  let files = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -rl red $dir
  assert files == f"{dir}/file\n"
}

test test_grep_pattern_files_only_matching_and_invalid_expression { |ctx|
  let root = test.temp_dir(ctx, name: "grep-patterns")?
  let file = fp"{root}/input"
  let patterns = fp"{root}/patterns"
  file.write("red-blue red\nother\n")
  patterns.write("red\nblue\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -Fo -f $patterns $file
  assert out == "red\nblue\nred\n"
  let invalid = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- "[" $file
  assert invalid.status.exited_with(2)
  assert invalid.stdout == ""
}

test test_grep_count_zero_binary_and_color { |ctx|
  let root = test.temp_dir(ctx, name: "grep-selection")?
  let file = fp"{root}/input"
  file.write("red\nblue\n")
  let zero = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -cm0 red $file
  assert zero.status.exited_with(1)
  assert zero.stdout == "0\n"
  let colored = run.bytes ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- --color=always red $file
  assert colored == b"\x1b[01;31m\x1b[Kred\x1b[m\x1b[K\n"
  file.write(b"red\0blue\n")
  let binary = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -F red $file
  assert binary == f"Binary file {file} matches\n"
}

test test_grep_byte_offsets_nul_names_and_recursive_filters { |ctx|
  let root = test.temp_dir(ctx, name: "grep-offset")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  let file = fp"{dir}/keep.txt"
  file.write("blue\nred red\n")
  fp"{dir}/skip.log".write("red\n")
  let offsets = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -Fbo red $file
  assert offsets == "5:red\n9:red\n"
  let names = run.bytes ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -rlZ --include=*.txt red $dir
  assert names == bytes.from_text(f"{file}\0")
}

test test_grep_errors_and_quiet_match_precedence { |ctx|
  let root = test.temp_dir(ctx, name: "grep-errors")?
  let file = fp"{root}/file"
  file.write("selected\n")
  let directory = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- selected $root
  assert directory.status.exited_with(2)
  let absent = fp"{root}/absent"
  let quiet = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -q selected $absent $file
  assert quiet.status.exited_with(0)
  assert quiet.stdout == ""
  let suppressed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -s selected $absent
  assert suppressed.status.exited_with(2)
  assert suppressed.stderr == ""
  let invalid = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- --count=ignored selected $file
  assert invalid.status.exited_with(2)
}

test test_grep_recursive_symlink_policy_and_loop_error { |ctx|
  let root = test.temp_dir(ctx, name: "grep-links")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  let file = fp"{dir}/file"
  file.write("needle\n")
  fp"{dir}/alias".symlink(to: p"file")
  let shallow = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -rl needle $dir
  assert shallow == f"{file}\n"
  fp"{dir}/loop".symlink(to: p".")
  let followed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -Rl needle $dir
  assert followed.status.exited_with(2)
  assert followed.stdout.split("\n").len() == 3
  assert followed.stderr != ""
}

test test_grep_files_without_match_exit_status { |ctx|
  let root = test.temp_dir(ctx, name: "grep-files-without-match")?
  let file = fp"{root}/input"
  file.write("asd\n")
  let listed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -L qwe $file
  assert listed.status.exited_with(0)
  assert listed.stdout == f"{file}\n"
  file.write("qwe\n")
  let matched = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -L qwe $file
  assert matched.status.exited_with(1)
  assert matched.stdout == ""
}
