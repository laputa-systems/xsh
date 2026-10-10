type TerminalRun = {status: Int, stderr: Str}

proc run_du_on_terminal(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[TerminalRun] {
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let root = test.temp_dir(ctx, name: "du-terminal")?
  let stdout = fp"{root}/stdout"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/du.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", stdout, fp"{pty.name}")
  let status = process.run(plan)?
  let stderr = unix.read_fd(pty.master, 8192)?.utf8()?
  Ok({status: status.exit_code()?, stderr: stderr.replace("\r\n", with: "\n")})
}

test test_du { |ctx|
  let target = test.temp_file(ctx, name: "du.txt", contents: b"abcdef")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- $target
  assert "du.txt" in output
  let apparent = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -b $target
  assert "6" in apparent
  let human = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -sh $target
  assert "K" in human
}

test test_du_recursive_all_and_total { |ctx|
  let root = test.temp_dir(ctx, name: "du-tree")?
  fp"{root}/a.txt".write("aaa")
  fp"{root}/sub".mkdir()
  fp"{root}/sub/b.txt".write("bb")
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -a -c $root
  assert f"{root}/a.txt" in output
  assert f"{root}/sub/b.txt" in output
  assert f"{root}/sub" in output
  assert "total" in output
  let summarized = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- --summarize --total $root
  assert f"{root}" in summarized
  assert "total" in summarized
}

test test_du_k_overrides_units_and_deduplicates_hard_links { |ctx|
  let root = test.temp_dir(ctx, name: "du-links")?
  let original = fp"{root}/original"
  let alias = fp"{root}/alias"
  original.write("abcdef")
  fs.link(original, alias)
  let units = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -mk $original
  let expected = (fs.stat(original)?.blocks_512 + 1) / 2
  assert units.stdout == f"{expected}\t{original}\n", units.stderr
  let deduplicated = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -b $original $alias
  assert deduplicated.stdout == f"6\t{original}\n", deduplicated.stderr
  let both = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -bl $original $alias
  assert both.stdout == f"6\t{original}\n6\t{alias}\n", both.stderr
}

test test_du_relative_symlink_and_error_continuation { |ctx|
  let root = test.temp_dir(ctx, name: "du-relative")?
  let file = fp"{root}/file"
  file.write("abcdef")
  let link = fp"{root}/link"
  link.symlink(to: p"file")
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/du.xsh"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-b", "missing", "link", "file"],
    root, {LC_ALL: "C"}, b"", out, err))?
  assert status.exited_with(1)
  assert out.read_text()? == "4\tlink\n6\tfile\n"
  assert "cannot access 'missing'" in err.read_text()?
}

test test_du_depth_inode_exclusion_and_dereference_invariants { |ctx|
  let root = test.temp_dir(ctx, name: "du-policy")?
  let tree = fp"{root}/tree"
  tree.mkdir()
  fp"{tree}/keep".write("abcdef")
  fp"{tree}/omit.tmp".write("excluded")
  fp"{tree}/child".mkdir()
  fp"{tree}/child/nested".write("nested")
  let summarized = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -b -d0 --exclude="*.tmp" $tree
  assert summarized.status.exited_with(0), summarized.stderr
  assert summarized.stdout == f"12\t{tree}\n"
  let inodes = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- --inodes -s --exclude="*.tmp" $tree
  assert inodes.status.exited_with(0), inodes.stderr
  assert inodes.stdout == f"4\t{tree}\n"
  let link = fp"{root}/link"
  link.symlink(to: p"tree")
  let followed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -bHs --exclude="*.tmp" $link
  assert followed.status.exited_with(0), followed.stderr
  assert followed.stdout == f"12\t{link}\n"
}

test test_du_repeated_options_verbose_exclusion_and_locale_decimal { |ctx|
  let root = test.temp_dir(ctx, name: "du-repeat")?
  let tree = fp"{root}/tree"
  tree.mkdir()
  fp"{tree}/keep".write("keep")
  fp"{tree}/skip".write("excluded")
  let output = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -s -A -A --verbose --exclude=skip $tree
  assert output.status.exited_with(0), output.stderr
  let lines = output.stdout.lines().collect()
  assert lines.len() == 2
  assert f"'{tree}/skip' ignored" in lines[0]
  assert lines[1] == f"1\t{tree}"

  let file = fp"{root}/large"
  file.write("")
  file.truncate(8500)
  let localized = run.capture --text LC_ALL=fr_FR.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -h -A $file
  assert localized.status.exited_with(0), localized.stderr
  assert localized.stdout == f"8,4K\t{file}\n"
}

test test_du_time_uses_latest_counted_entry { |ctx|
  let root = test.temp_dir(ctx, name: "du-time")?
  let file = fp"{root}/file"
  file.write("data")
  fs.set_times(file, mtime_ns: 1420115640000000000)
  let output = run.capture --text TZ=UTC0 ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -b --time --time-style="+%Y-%m-%d" $file
  assert output.status.exited_with(0), output.stderr
  assert output.stdout == f"4\t2015-01-01\t{file}\n"
  fs.set_times(file, atime_ns: 1420115640000000000)
  for selector in ["atim", "a", "acce"] {
    let abbreviated = run.capture --text TZ=UTC0 ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -b --time=$selector --time-style="+%Y-%m-%d" $file
    assert abbreviated.status.exited_with(0), abbreviated.stderr
    assert abbreviated.stdout == f"4\t2015-01-01\t{file}\n"
  }
}

test test_du_long_bytes_selector_and_filename_list_errors { |ctx|
  let root = test.temp_dir(ctx, name: "du-inputs")?
  let file = fp"{root}/file"
  file.write("abcdef")
  let apparent = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- --bytes $file
  assert apparent.status.exited_with(0), apparent.stderr
  assert apparent.stdout == f"6\t{file}\n"
  let failed = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- --files0-from=$root
  assert failed.status.exited_with(1)
  assert failed.stderr == f"du: {root}: read error: Is a directory\n"
  let invalid = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- --threshold=-0 $file
  assert invalid.status.exited_with(1)
  assert invalid.stderr == "du: invalid --threshold argument '-0'\n"
}

test test_du_null_filename_lists_human_sizes_and_logical_cycles { |ctx|
  let root = test.temp_dir(ctx, name: "du-boundaries")?
  let file = fp"{root}/file"
  file.write("abcdef")
  let list = fp"{root}/names"
  list.write(f"{file}\0{file}\0")
  let names = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -b0 --files0-from=$list
  assert names.status.exited_with(0), names.stderr
  assert names.stdout == f"6\t{file}\0"
  file.truncate(8500)
  let human = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- --apparent-size -h $file
  assert human.status.exited_with(0), human.stderr
  assert human.stdout == f"8.4K\t{file}\n"
  let block_human = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -B human-readable $file
  assert block_human.status.exited_with(0), block_human.stderr
  let block_si = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -B si $file
  assert block_si.status.exited_with(0), block_si.stderr
  let tree = fp"{root}/tree"
  tree.mkdir()
  fp"{tree}/payload".write("data")
  fp"{tree}/self".symlink(to: p".")
  let cycle = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -bLs $tree
  assert cycle.status.exited_with(0), cycle.stderr
  assert cycle.stdout == f"4\t{tree}\n"
  assert cycle.stderr == ""
}

test test_du_missing_exclude_file_continues_operands { |ctx|
  let root = test.temp_dir(ctx, name: "du-exclude-error")?
  let file = fp"{root}/file"
  file.write("data")
  let missing = fp"{root}/missing"
  let output = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -b -X $missing $file
  assert output.status.exited_with(1)
  assert output.stdout == f"4\t{file}\n"
  assert output.stderr == "du: No such file or directory\n"
}

test test_du_size_errors_point_into_terminal_arguments { |ctx|
  let unknown = run_du_on_terminal(ctx, ["-B", "1fb"])?
  assert unknown.status == 1
  assert unknown.stderr == """du: invalid suffix in --block-size argument '1fb'
   ╭─[ du:1:8 ]
   │
 1 │ du -B 1fb
   │        ─┬
   │         ╰── not a known unit
   │
   │ Help: a size is a number and an optional unit: K, M, G and so on for 1024, KB, MB, GB for 1000
───╯
""", unknown.stderr

  let attached = run_du_on_terminal(ctx, ["--block-size=1fb"])?
  assert attached.status == 1
  assert "du:1:18" in attached.stderr, attached.stderr
  assert "not a known unit" in attached.stderr, attached.stderr

  let zero = run_du_on_terminal(ctx, ["-B", "0"])?
  assert zero.status == 1
  assert "du:1:7" in zero.stderr, zero.stderr
  assert ! ("not a known unit" in zero.stderr), zero.stderr

  let rejected_threshold = run_du_on_terminal(ctx, ["-t", "-0"])?
  assert rejected_threshold.status == 1
  assert "du:1:7" in rejected_threshold.stderr, rejected_threshold.stderr

  let unknown_threshold_unit = run_du_on_terminal(ctx, ["-t", "-1fb"])?
  assert unknown_threshold_unit.status == 1
  assert "du:1:9" in unknown_threshold_unit.stderr, unknown_threshold_unit.stderr
  assert "not a known unit" in unknown_threshold_unit.stderr, unknown_threshold_unit.stderr
}

test test_du_walks_long_nested_paths { |ctx|
  let root = test.temp_dir(ctx, name: "du-long-path")?
  let tree = fp"{root}/tree"
  tree.mkdir()
  var deepest = tree
  var component = ""
  for _ in range(10) { component = f"{component}0123456789" }

  for index in range(15) {
    let child = fp"{deepest}/{component}{index}"
    child.mkdir()
    deepest = child
  }

  let payload = fp"{deepest}/payload"
  payload.write("content")

  let summarized = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -bs $tree
  assert summarized.status.exited_with(0), summarized.stderr
  assert summarized.stdout == f"7\t{tree}\n"

  let all = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -ab $tree
  assert all.status.exited_with(0), all.stderr
  assert f"7\t{payload}\n" in all.stdout
}

test test_du_preserves_names_with_spaces_and_null_records { |ctx|
  let root = test.temp_dir(ctx, name: "du-quote")?
  let file = fp"{root}/a b"
  file.write("abc")

  let quoted = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -b $file
  assert quoted.status.exited_with(0), quoted.stderr
  assert quoted.stdout == f"3\t{file}\n"

  let nul = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -- -b0 $file
  assert nul.status.exited_with(0), nul.stderr
  assert nul.stdout == f"3\t{file}\0"
}

test test_du_files0_from_reports_gnu_open_and_operand_errors { |ctx|
  let root = test.temp_dir(ctx, name: "du-files0-errors")?
  let missing = fp"{root}/missing"
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/du.xsh"
  let missing_run = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), f"--files0-from={missing}"],
    root, {LC_ALL: "C"}, b"", out, err))?
  assert missing_run.exited_with(1)
  assert err.read_text()? == f"du: cannot open '{missing}' for reading: No such file or directory\n"
  let extra_run = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--files0-from=-", "no-such"],
    root, {LC_ALL: "C"}, b"", out, err))?
  assert extra_run.exited_with(1)
  assert err.read_text()? == "du: extra operand 'no-such'\nfile operands cannot be combined with --files0-from\nTry 'du --help' for more information.\n"
}

test test_du_dotdot_operand_is_its_own_anchor { |ctx|
  let root = test.temp_dir(ctx, name: "du-dotdot")?
  let sub = fp"{root}/sub"
  sub.mkdir()
  fp"{sub}/t".mkdir()
  fp"{sub}/t/file".write("abc")
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/du.xsh"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-s", ".."],
    sub, {LC_ALL: "C"}, b"", out, err))?
  assert status.exited_with(0), err.read_text()?
  assert err.read_text()? == ""
  let lines = out.read_text()?.lines().collect()
  assert lines.len() == 1, out.read_text()?
  assert lines[0].ends_with("\t.."), lines[0]
}

test test_du_walks_long_paths_from_unreadable_cwd { |ctx|
  let root = test.temp_dir(ctx, name: "du-path-limit")?
  let tree = fp"{root}/tree"
  tree.mkdir()
  let inaccessible = fp"{root}/inaccessible"
  inaccessible.mkdir()
  var component = ""
  for _ in range(20) { component = f"{component}0123456789" }
  let first = fp"{tree}/{component}20"
  var deep_root = fs.open_root(tree)?
  for remaining in range(20) {
    let name = fp"{component}{20 - remaining}"
    deep_root.mkdir(name)
    let child = deep_root.open_root(name)?
    deep_root.close()
    deep_root = child
  }
  defer deep_root.close()
  deep_root.write(p"payload", "content")
  assert deep_root.children(p".")?.children == [p"payload"]
  assert deep_root.metadata(p"payload")?.size == 7

  let result = run.capture --text sh -c r"""
cd "$1" || exit
chmod 000 .
"$2" "$3" -- -s "$4"
status=$?
chmod 700 .
exit "$status"
""" sh $inaccessible ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" $first

  assert result.status.exited_with(0), result.stderr
  assert result.stdout.ends_with(f"{first}\n")
}

test test_du_reports_children_blocked_by_directory_search_permissions { |ctx|
  if unix.id()?.euid == 0 { test.skip("root bypasses directory permission checks") }
  let root = test.temp_dir(ctx, name: "du-denied")?
  let no_search = fp"{root}/d/no-x"
  let blocked_child = fp"{no_search}/y"
  blocked_child.mkdir(parents: true)
  defer { no_search.chmod(0o700) }
  no_search.chmod(0o600)
  let script = fp"{ctx.core_dir}/du.xsh"
  let inaccessible_out = fp"{root}/inaccessible.out"
  let inaccessible_err = fp"{root}/inaccessible.err"
  let inaccessible_status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-s", "d"],
    root, {LC_ALL: "C"}, b"", inaccessible_out, inaccessible_err))?
  assert inaccessible_status.exited_with(1)
  assert inaccessible_err.read_text()? == "du: cannot access 'd/no-x/y': Permission denied\n"
}

test test_du_reports_unreadable_directories { |ctx|
  if unix.id()?.euid == 0 { test.skip("root bypasses directory permission checks") }
  let root = test.temp_dir(ctx, name: "du-unreadable")?
  let script = fp"{ctx.core_dir}/du.xsh"
  let no_read = fp"{root}/subdir/links"
  no_read.mkdir(parents: true)
  fp"{no_read}/child".write("content")
  defer { no_read.chmod(0o700) }
  no_read.chmod(0o300)
  let unreadable_out = fp"{root}/unreadable.out"
  let unreadable_err = fp"{root}/unreadable.err"
  let unreadable_status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "subdir/links"],
    root, {LC_ALL: "C"}, b"", unreadable_out, unreadable_err))?
  assert unreadable_status.exited_with(1)
  assert unreadable_err.read_text()? == "du: cannot read directory 'subdir/links': Permission denied\n"
}
