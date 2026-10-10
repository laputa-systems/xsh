test test_find_boolean_expression_prune_and_print0 { |ctx|
  let root = test.temp_dir(ctx, name: "find-expression")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  fp"{dir}/keep.txt".write("keep")
  fp"{dir}/skip".mkdir()
  fp"{dir}/skip/hidden.txt".write("hidden")
  let out = run.bytes ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $dir -name skip -prune -o -type f -name "*.txt" -print0
  assert out == bytes.from_text(f"{dir}/keep.txt\0")
}

test test_find_exec_and_depth_first_delete { |ctx|
  let root = test.temp_dir(ctx, name: "find-actions")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  fp"{dir}/file".write("contents")
  let observed1 = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $dir -type f -exec printf "<%s>\n" "{}" ";"
  assert observed1 == f"<{dir}/file>\n"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $dir -delete
  assert ! dir.exists()?
}

test test_find_parentheses_negation_depth_and_predicates { |ctx|
  let root = test.temp_dir(ctx, name: "find-depth")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  fp"{dir}/A.TXT".write("1234")
  fp"{dir}/empty".write("")
  fp"{dir}/sub".mkdir()
  fp"{dir}/sub/hidden".write("hidden")
  let valid = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $dir -maxdepth 1 "(" -iname "*.txt" -o -type f -empty ")" -print
  assert valid.split("\n").len() == 3
  let sized = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $dir -type f -size 4c -print
  assert sized == f"{dir}/A.TXT\n"
}

test test_find_batched_exec_and_execdir { |ctx|
  let root = test.temp_dir(ctx, name: "find-batch")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  fp"{dir}/one".write("one")
  fp"{dir}/two".write("two")
  let batched = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $dir -type f -exec sh -c "printf '%s\\n' \"$#\"" sh "{}" "+"
  assert batched == "2\n"
  let local = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $dir -type f -execdir /bin/pwd ";"
  assert local == f"{dir}\n{dir}\n"
}

test test_find_explicit_posix_regex_and_delete_prune_guard { |ctx|
  let root = test.temp_dir(ctx, name: "find-regex")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  fp"{dir}/red.txt".write("red")
  fp"{dir}/blue.log".write("blue")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $dir -regextype posix-extended -regex ".*/(red|blue)\\.txt"
  assert out == f"{dir}/red.txt\n"
  let dangerous = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $dir -prune -delete
  assert dangerous.status.exited_with(1)
  assert fp"{dir}/red.txt".exists()?
}

test test_find_metadata_survives_delete_for_later_actions { |ctx|
  let root = test.temp_dir(ctx, name: "find-delete-print")?
  let file = fp"{root}/file"
  file.write("contents")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $file -delete -type f -print
  assert out == f"{file}\n"
  assert ! file.exists()?
}

test test_find_symbolic_and_octal_permissions { |ctx|
  let root = test.temp_dir(ctx, name: "find-permissions")?
  let file = fp"{root}/file"
  file.write("contents")
  file.chmod(0o640)
  let exact = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $file -perm "u=rw,g=r" -print
  assert exact == f"{file}\n"
  let all = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $file -perm "-u+r,g+r" -print
  assert all == f"{file}\n"
  let any = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/find.xsh" -- $file -perm /111 -print
  assert any.status.exited_with(0)
  assert any.stdout == ""
}

use core.lib.findutils_fixture as fixture

# Every command line of the recorded GNU find corpus, in six shares that run
# in parallel; the corpus and the fixture tree are described in
# tests/data/findutils/regen.xsh.
proc check_share(ctx: TestContext, part: Int) [fs, process, time, error] -> Result[Unit, Error] {
  let root = test.temp_dir(ctx, name: f"find-corpus-{part}")?
  let work = test.temp_dir(ctx, name: f"find-corpus-out-{part}")?
  fixture.build(root)?
  fixture.build_hostile(root)?
  defer fixture.restore(root)
  let corpus = fp"{ctx.core_dir}/tests/data/findutils/find.jsonl"
  let failures = fixture.run_corpus(ctx.xsh_bin, fp"{ctx.core_dir}/find.xsh", corpus, root, work, part, 6)?
  assert failures.is_empty(), failures.join("\n")
  Ok()
}

test test_find_matches_recorded_gnu_output_0 { |ctx| check_share(ctx, 0)? }
test test_find_matches_recorded_gnu_output_1 { |ctx| check_share(ctx, 1)? }
test test_find_matches_recorded_gnu_output_2 { |ctx| check_share(ctx, 2)? }
test test_find_matches_recorded_gnu_output_3 { |ctx| check_share(ctx, 3)? }
test test_find_matches_recorded_gnu_output_4 { |ctx| check_share(ctx, 4)? }
test test_find_matches_recorded_gnu_output_5 { |ctx| check_share(ctx, 5)? }
