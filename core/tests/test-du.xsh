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
  assert output.stderr == f"du: {missing}: No such file or directory\n"
}
