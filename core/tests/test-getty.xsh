test test_getty_requires_baud_and_tty { |ctx|
  let err = test.temp_path(ctx, name: "getty.err")
  let result = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/getty.xsh" -n -i 2> $err
  assert ! result.exited_with(0)
  assert "missing operand" in err.read_text()?
}
