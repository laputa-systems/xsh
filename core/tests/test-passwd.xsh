test test_passwd_rejects_extra_operands [fs, process, error] { |ctx|
  let err = test.temp_path(ctx, name: "passwd.err")
  let result = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/passwd.xsh" -- user extra 2> $err
  ! result.exited_with(0)
  "extra operand" in err.read_text()?
}
