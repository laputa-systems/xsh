test test_passwd_rejects_extra_operands { |ctx|
  let err = test.temp_path(ctx, name: "passwd.err")
  let result = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/passwd.xsh" user extra 2> $err
  assert ! result.exited_with(0)
  assert "extra operand" in err.read_text()?
}
