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

test test_mkdir_default_context_flag_is_silent_without_selinux { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir-default-context")?
  let target = fp"{root}/a/b"
  let created = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- -Z -p $target
  assert created.status.exited_with(0), created.stderr
  assert created.stdout == "" and created.stderr == ""
  assert target.is_dir()?
}

test test_mkdir_default_context_flag_takes_no_value { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir-default-context-operand")?
  let label_text = fp"{root}/unconfined_u:object_r:user_tmp_t:s0"
  let target = fp"{root}/target"
  let created = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- -Z $label_text $target
  assert created.status.exited_with(0), created.stderr
  assert created.stderr == ""
  assert label_text.is_dir()?
  assert target.is_dir()?
}

test test_mkdir_default_context_flag_rejects_attached_letters { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir-default-context-letters")?
  let target = fp"{root}/target"
  let refused = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- -Zfoo $target
  assert refused.status.exited_with(1)
  assert refused.stdout == ""
  assert refused.stderr == "mkdir: invalid option -- 'f'\nTry 'mkdir --help' for more information.\n"
  assert ! target.exists()?
}

test test_mkdir_context_without_value_is_silent { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir-context-bare")?
  let bare = fp"{root}/bare"
  let abbreviated = fp"{root}/abbreviated"
  let created = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- --context $bare
  assert created.status.exited_with(0), created.stderr
  assert created.stderr == ""
  assert bare.is_dir()?
  let abbrev = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- --cont $abbreviated
  assert abbrev.status.exited_with(0), abbrev.stderr
  assert abbrev.stderr == ""
  assert abbreviated.is_dir()?
}

test test_mkdir_context_value_warns_and_creates { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir-context-value")?
  let target = fp"{root}/target"
  let created = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- --context=unconfined_u:object_r:user_tmp_t:s0 $target
  assert created.status.exited_with(0), created.stderr
  assert created.stdout == ""
  assert created.stderr == "mkdir: warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel\n"
  assert target.is_dir()?
}

test test_mkdir_context_warns_once_per_valued_occurrence { |ctx|
  let root = test.temp_dir(ctx, name: "mkdir-context-count")?
  let two = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- --context=a --context=b fp"{root}/first"
  assert two.status.exited_with(0), two.stderr
  assert two.stderr == "mkdir: warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel\nmkdir: warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel\n"
  let empty_value = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- --context= fp"{root}/second"
  assert empty_value.status.exited_with(0), empty_value.stderr
  assert empty_value.stderr == "mkdir: warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel\n"
  let bare_after = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- --context=x --context fp"{root}/third"
  assert bare_after.status.exited_with(0), bare_after.stderr
  assert bare_after.stderr == "mkdir: warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel\n"
}

test test_mkdir_context_warning_precedes_help_and_is_not_reached_after_help { |ctx|
  let warned = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- --context=x --help
  assert warned.status.exited_with(0), warned.stderr
  assert warned.stderr == "mkdir: warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel\n"
  assert warned.stdout.starts_with("Usage: mkdir")
  let quiet = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mkdir.xsh" -- --help --context=x
  assert quiet.status.exited_with(0), quiet.stderr
  assert quiet.stderr == ""
}
