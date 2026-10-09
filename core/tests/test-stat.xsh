test test_stat { |ctx|
  let target = test.temp_file(ctx, name: "stat.txt", contents: b"hello")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" $target ?
  assert "regular file" in output
  assert "Size: 5" in output
  let formatted = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -c "%s %F %n" $target ?
  assert "5 regular file" in formatted
  assert "stat.txt" in formatted
  let modes = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -c "%a %A %U %G" $target ?
  assert "rw" in modes
}

test test_stat_gnu_format_metadata_and_mount_point { |ctx|
  let target = test.temp_file(ctx, name: "stat-format.txt", contents: b"12345")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -c "%s %F %b %B %u %g %h %i %m %n" $target ?
  assert output.starts_with("5 regular file ")
  assert " 512 " in output
  assert target.display() in output
}

test test_stat_symlink_follow_option { |ctx|
  let root = test.temp_dir(ctx, name: "stat-follow")?
  let file = fp"{root}/file"
  let link = fp"{root}/link"
  file.write("payload")
  fs.symlink(p"file", link)

  let link_kind = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -c "%F" $link ?
  let followed_kind = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -L -c "%F" $link ?
  assert link_kind.trim() == "symbolic link"
  assert followed_kind.trim() == "regular file"
}

test test_stat_file_system_format { |ctx|
  let target = test.temp_dir(ctx, name: "stat-fs-format")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -f -c "%b %c %i %l %n %s %S %t %T" $target ?
  assert target.display() in output
  assert output.words().len() == 9
  let terse = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -f -t $target ?
  assert terse.starts_with(f"{target} ")
  assert terse.words().len() == 11
  let filesystem_id = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -f -c "%i" $target ?
  assert terse.words()[1] == filesystem_id.trim()
  let spaced = test.temp_file(ctx, name: "stat fs format", contents: b"")?
  let quoted = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/stat.xsh" -f -t $spaced ?
  assert quoted.starts_with("'")
  assert "stat fs format" in quoted
  assert "' " in quoted
}
