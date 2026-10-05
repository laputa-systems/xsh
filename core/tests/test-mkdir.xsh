test test_mkdir { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir")?
  let nested = fp"{root}/a/b"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- -p -m 700 $nested
  assert nested.exists()?
  assert nested.metadata()?.mode % 512 == 448
}

test test_mkdir_all_octal_modes_and_existing_parents { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir-mode")?
  let target = fp"{root}/target"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- -m 1750 $target
  assert target.metadata()?.mode.bit_and(0o7777) == 0o1750
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- -p -m 700 $target
  assert target.metadata()?.mode.bit_and(0o7777) == 0o1750
}

test test_mkdir_symbolic_mode { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir-symbolic")?
  let target = fp"{root}/target"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- -m "u=rwx,g=rx,o=" $target
  assert target.metadata()?.mode.bit_and(0o7777) == 0o750
}

test test_mkdir_continues_after_failure { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir-errors")?
  let existing = fp"{root}/existing"
  existing.mkdir()
  let good = fp"{root}/good"
  let failed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- $existing $good
  assert failed.status.exited_with(1)
  assert good.exists()?
  let invalid = fp"{root}/invalid"
  let refused = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- -m "u=invalid" $invalid
  assert refused.status.exited_with(1)
  assert ! invalid.exists()?
}

test test_mkdir_parent_inherits_setgid { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir-setgid")?
  root.chmod(0o2770)
  let final = fp"{root}/parent/final"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- -p $final
  assert fp"{root}/parent".metadata()?.mode.bit_and(0o2000) == 0o2000
  assert final.metadata()?.mode.bit_and(0o2000) == 0o2000
}

test test_mkdir_parents_preserves_dot_components { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir-dot")?
  let target = fp"{root}/a/."
  let created = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- -p $target
  assert created.status.exited_with(0), created.stderr
  assert fp"{root}/a".is_dir()?
}

test test_mkdir_empty_operand_fails { |ctx|
  let failed = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- ""
  assert failed.status.exited_with(1)
  assert failed.stderr == "mkdir: cannot create directory '': No such file or directory\n"
}
