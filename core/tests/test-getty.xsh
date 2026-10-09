test test_getty_requires_baud_and_tty { |ctx|
  let err = test.temp_path(ctx, name: "getty.err")
  let result = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/getty.xsh" -n -i 2> $err
  assert ! result.exited_with(0)
  assert "missing operand" in err.read_text()?
}

test test_getty_rejects_unavailable_terminal_options { |ctx|
  for option in ["-h", "--flow-control", "-L", "--local-line", "--local-line=always", "-m", "--extract-baud", "-w", "--wait-cr", "-t", "--timeout"] {
    let err = test.temp_path(ctx, name: "getty-unsupported.err")
    let result = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/getty.xsh" $option 2> $err
    assert ! result.exited_with(0), option
    assert "is not supported" in err.read_text()?, option
  }
}
