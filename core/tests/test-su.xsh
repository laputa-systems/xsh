test test_su_returns_failure_for_unknown_user { |ctx|
  let err = test.temp_path(ctx, name: "su.err")
  let result = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/su.xsh" -- xsh-no-such-user-for-test 2> $err
  result.exited_with(1)
  "su: user was not found" in err.read_text()?
}
