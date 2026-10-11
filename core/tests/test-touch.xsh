test test_touch { |ctx|
  let root = test.temp_dir(ctx, name: "touch")?
  let target = fp"{root}/created.txt"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- $target
  assert target.exists()?
  let missing = fp"{root}/missing.txt"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -c $missing
  assert ! missing.exists()?
}

test test_touch_sets_a_date_outside_the_nanosecond_range { |ctx|
  let root = test.temp_dir(ctx, name: "touch-year-zero")?
  let target = fp"{root}/target"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -d 0000-01-01 $target
  assert target.exists()?
  assert fs.stat(target)?.mtime_ns < -2000000000000000000
}

test test_touch_accepts_non_utf8_operand { |ctx|
  let root = test.temp_dir(ctx, name: "touch-raw-path")?
  let target = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/touch.xsh"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, p"--", script, p"--", target], root, {LC_ALL: "C"}, b"", stdout, stderr))?
  assert status.exited_with(0), stderr.read_text()?
  assert fs.stat(target) is Ok(_)

  let reference = Path.parse_bytes(bytes.concat([root.bytes(), b"/reference\xfe"]))?
  reference.write("reference")
  fs.set_times(reference, mtime_ns: 2000000002)
  let relative_target = fp"{root}/relative-target"
  relative_target.write("target")
  fs.set_times(relative_target, mtime_ns: 1000000001)
  let reference_status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, p"--", script, p"-m", p"-r", reference, relative_target], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert reference_status.exited_with(0), stderr.read_text()?
  assert fs.stat(relative_target)?.mtime_ns == 2000000002
}

test test_touch_selected_reference_times { |ctx|
  let root = test.temp_dir(ctx, name: "touch-selected")?
  let reference = fp"{root}/reference"
  let target = fp"{root}/target"
  reference.write("reference")
  target.write("content")
  fs.set_times(reference, atime_ns: 1000000001, mtime_ns: 2000000002)
  fs.set_times(target, atime_ns: 3000000003, mtime_ns: 4000000004)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -a -r $reference $target
  assert fs.stat(target)?.atime_ns == 1000000001
  assert fs.stat(target)?.mtime_ns == 4000000004
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -m --reference=$reference $target
  assert fs.stat(target)?.mtime_ns == 2000000002
  assert target.read_text()? == "content"
}

test test_touch_nofollow_and_directory { |ctx|
  let root = test.temp_dir(ctx, name: "touch-link")?
  let file = fp"{root}/file"
  let link = fp"{root}/link"
  file.write("content")
  link.symlink(to: p"file")
  fs.set_times(file, mtime_ns: 1000000001)
  fs.set_times(link, mtime_ns: 2000000002)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -h -m $link
  assert fs.stat(file)?.mtime_ns == 1000000001
  assert fs.stat(link)?.mtime_ns > 2000000002
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- $root
  let missing = fp"{root}/missing"
  let absent = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -h $missing
  assert absent.status.exited_with(1)
  assert "setting times of" in absent.stderr
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -ch $missing
  assert ! missing.exists()?
}

test test_touch_continues_after_failure_and_checks_reference_first { |ctx|
  let root = test.temp_dir(ctx, name: "touch-errors")?
  let bad = fp"{root}/missing/child"
  let good = fp"{root}/good"
  let failed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- $bad $good
  assert failed.status.exited_with(1)
  assert good.exists()?
  let untouched = fp"{root}/untouched"
  let reference = fp"{root}/absent-reference"
  let absent = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -r $reference $untouched
  assert absent.status.exited_with(1)
  assert ! untouched.exists()?
}

test test_touch_dash_updates_stdout_descriptor { |ctx|
  let root = test.temp_dir(ctx, name: "touch-stdout")?
  let reference = fp"{root}/reference"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  reference.write("reference")
  fs.set_times(reference, atime_ns: 1000000001, mtime_ns: 2000000002)
  let script = fp"{ctx.core_dir}/touch.xsh"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-h", "-r", reference.display(), "-"],
    root, {}, b"", stdout, stderr))?
  assert status.exited_with(0), stderr.read_text()?
  assert fs.stat(stdout)?.mtime_ns == 2000000002
  assert ! fp"{root}/-".exists()?
}

test test_touch_calendar_and_epoch_dates { |ctx|
  let root = test.temp_dir(ctx, name: "touch-dates")?
  let calendar = fp"{root}/calendar"
  let compact = run.capture --text TZ=UTC0 ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -t 201501011234 $calendar
  assert compact.status.exited_with(0), compact.stderr
  assert fs.stat(calendar)?.mtime_ns == 1420115640000000000
  let epoch = fp"{root}/epoch"
  let fractional = run.capture --text TZ=UTC0 ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -d "@123.000000456" $epoch
  assert fractional.status.exited_with(0), fractional.stderr
  assert fs.stat(epoch)?.mtime_ns == 123000000456
}

test test_touch_relative_reference_preserves_each_time { |ctx|
  let root = test.temp_dir(ctx, name: "touch-relative")?
  let reference = fp"{root}/reference"
  let target = fp"{root}/target"
  reference.write("reference")
  fs.set_times(reference, atime_ns: 1420115640000000000, mtime_ns: 1420202040000000000)
  let relative = run.capture --text TZ=UTC0 ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -d "+5 days" -r $reference $target
  assert relative.status.exited_with(0), relative.stderr
  assert fs.stat(target)?.atime_ns == 1420547640000000000
  assert fs.stat(target)?.mtime_ns == 1420634040000000000
}

test test_touch_negative_relative_fortnight { |ctx|
  let root = test.temp_dir(ctx, name: "touch-fortnight")?
  let reference = fp"{root}/reference"
  let target = fp"{root}/target"
  reference.write("reference")
  fs.set_times(reference, atime_ns: 1420115640000000000, mtime_ns: 1420202040000000000)
  let changed = run.capture --text TZ=UTC0 ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -d "-1 fortnight" -r $reference $target
  assert changed.status.exited_with(0), changed.stderr
  assert fs.stat(target)?.atime_ns == 1420115640000000000 - 1209600000000000
  assert fs.stat(target)?.mtime_ns == 1420202040000000000 - 1209600000000000
}

test test_touch_compact_leap_second { |ctx|
  let root = test.temp_dir(ctx, name: "touch-leap")?
  let target = fp"{root}/target"
  let changed = run.capture --text TZ=UTC0 ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- -t 197001010000.60 $target
  assert changed.status.exited_with(0), changed.stderr
  assert fs.stat(target)?.mtime_ns == 60000000000
}

test test_touch_missing_file_trailing_slash { |ctx|
  let root = test.temp_dir(ctx, name: "touch-slash")?
  let target = f"{root}/missing/"
  let failed = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- $target
  assert failed.status.exited_with(1)
  assert failed.stderr == f"touch: setting times of '{target}': No such file or directory\n"
  assert ! fp"{target}".exists()?
}

# The operands are relative to the test directory, so the obsolete form is
# matched against names the script would otherwise create.
test test_touch_obsolete_operand_uses_posix_version_and_yearly_range { |ctx|
  let root = test.temp_dir(ctx, name: "touch-obsolete")?
  cd $root {
    let posix = run.capture --text env _POSIX2_VERSION=199209 POSIXLY_CORRECT=1 LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- "0101000099" "with-year"
    assert posix.status.exited_with(0), posix.stderr
    assert posix.stderr == ""
    let no_year = run.capture --text env _POSIX2_VERSION=199209 POSIXLY_CORRECT=1 LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- "01010000" "no-year"
    assert no_year.status.exited_with(0), no_year.stderr
    assert ! fp"{root}/01010000".exists()?
    let early_year = run.capture --text env _POSIX2_VERSION=199209 POSIXLY_CORRECT=1 LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- "0101000068" "after"
    assert early_year.status.exited_with(0), early_year.stderr
    let modern = run.capture --text env _POSIX2_VERSION=200809 LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- "01010000" "modern"
    assert modern.status.exited_with(0), modern.stderr
  }
  assert ! fp"{root}/0101000099".exists()?
  assert fp"{root}/with-year".exists()?
  assert fp"{root}/no-year".exists()?
  assert fp"{root}/0101000068".exists()?
  assert fp"{root}/after".exists()?
  assert fp"{root}/01010000".exists()?
}

test test_touch_obsolete_operand_warns_without_posixly_correct { |ctx|
  let root = test.temp_dir(ctx, name: "touch-obsolete-warning")?
  cd $root {
    let warned = run.capture --text env -u POSIXLY_CORRECT _POSIX2_VERSION=199209 LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -- "01010000" "target"
    assert warned.status.exited_with(0), warned.stderr
    assert warned.stderr.starts_with("touch: warning: 'touch 01010000' is obsolete; use 'touch -t "), warned.stderr
    assert warned.stderr.ends_with(".00'\n"), warned.stderr
  }
  assert fp"{root}/target".exists()?
}
