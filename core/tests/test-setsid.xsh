test test_setsid_fork_wait_returns_command_status_and_creates_session { |ctx|
  let root = test.temp_dir(ctx, name: "setsid")?
  let child = fp"{root}/child.xsh"
  child.write("assert process.session_id(process.current_pid()?)? == process.current_pid()?\nassert process.group_id(process.current_pid()?)? == process.current_pid()?\nexit 7\n")
  for flags in [["-f", "-w"], ["-w"]] {
    let argv: List[Union[Str, Path]] = collect { for flag in flags { yield flag }; yield ctx.xsh_bin; yield child }
    let result = test.run_script(ctx, fp"{ctx.core_dir}/setsid.xsh".read_text()?, argv, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "setsid")?
    assert result.status == 7, result.stderr
  }
}

test test_setsid_wait_preserves_signal_exit_status { |ctx|
  let result = test.run_script(ctx, fp"{ctx.core_dir}/setsid.xsh".read_text()?, ["-f", "-w", "/bin/sh", "-c", "kill -TERM $$"], {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "setsid")?
  assert result.status == 143, result.stderr
}
