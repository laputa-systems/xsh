type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/mknod.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_mknod_fifo_and_invalid_device_number { |ctx|
  let root = test.temp_dir(ctx, name: "mknod")?
  assert run_applet(ctx, root, ["-m", "640", "pipe", "p"])?.status == 0
  assert fs.stat(fp"{root}/pipe")?.kind == "fifo"
  assert fs.stat(fp"{root}/pipe")?.mode.bit_and(0o777) == 0o640
  assert run_applet(ctx, root, ["node", "c", "bad", "3"])?.status == 1
  assert ! fp"{root}/node".exists()?
}


test test_mknod_type_mnemonics_and_operand_diagnostics { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-types")?
  assert run_applet(ctx, root, ["pipe", "pipe"])?.status == 0
  assert fs.stat(fp"{root}/pipe")?.kind == "fifo"
  assert "Special files require major and minor device numbers." in run_applet(ctx, root, ["node", "c"])?.stderr
  assert "Fifos do not have major and minor device numbers." in run_applet(ctx, root, ["other", "p", "1", "2"])?.stderr
}


test test_mknod_invalid_device_numbers_report_invalid_value { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-invalid-device-number")?
  for args in [["node", "c", "c", "1"], ["node", "c", "1", "c"], ["node", "c", "4294967296", "1"]] {
    let result = run_applet(ctx, root, args)?
    let invalid = if args[2] == "1" { args[3] } else { args[2] }
    assert result.status == 1
    assert f"invalid value '{invalid}'" in result.stderr, result.stderr
    assert ! fp"{root}/node".exists()?
  }
}

test test_mknod_unknown_long_option_reports_unexpected_argument { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-unknown-option")?
  let result = run_applet(ctx, root, ["--foo"])?
  assert result.status == 1
  assert "unexpected argument '--foo' found" in result.stderr, result.stderr
  assert result.stdout == ""
}
