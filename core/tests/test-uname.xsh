type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/uname.xsh by its real path (so the invoked name is uname and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(ctx: TestContext, args: List[Str], vars: Record = {LC_ALL: "C"}, stdin: Bytes = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "uname")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/uname.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_uname_default_is_the_kernel_name { |ctx|
  let result = applet_run(ctx, [])?
  assert result.status == 0
  assert result.stdout == f"{system.uname()?.sysname}\n"
}

test test_uname_selects_fields_in_fixed_order { |ctx|
  let info = system.uname()?

  assert applet_run(ctx, ["-n"])?.stdout == f"{info.nodename}\n"
  assert applet_run(ctx, ["-r"])?.stdout == f"{info.release}\n"
  assert applet_run(ctx, ["-v"])?.stdout == f"{info.version}\n"
  assert applet_run(ctx, ["-m"])?.stdout == f"{info.machine}\n"
  assert applet_run(ctx, ["-mn"])?.stdout == f"{info.nodename} {info.machine}\n"
  assert applet_run(ctx, ["--machine", "--kernel-name", "--sysname"])?.stdout == f"{info.sysname} {info.machine}\n"
  assert applet_run(ctx, ["--release", "--kernel-release"])?.stdout == f"{info.release}\n"
}

test test_uname_all_omits_unknown_processor_and_platform { |ctx|
  let info = system.uname()?
  let expected = f"{info.sysname} {info.nodename} {info.release} {info.version} {info.machine} GNU/Linux\n"

  assert applet_run(ctx, ["-a"])?.stdout == expected
  assert applet_run(ctx, ["-p", "-a"])?.stdout == expected
  assert applet_run(ctx, ["-pi"])?.stdout == "unknown unknown\n"
  assert applet_run(ctx, ["-o"])?.stdout == "GNU/Linux\n"
}

test test_uname_all_labeled_prints_one_field_per_line { |ctx|
  let info = system.uname()?
  let labeled = applet_run(ctx, ["-A"])?.stdout

  assert labeled == f"Kernel name: {info.sysname}\nNode name: {info.nodename}\nKernel release: {info.release}\nKernel version: {info.version}\nMachine: {info.machine}\nOperating system: GNU/Linux\n"
  assert applet_run(ctx, ["--all-labeled"])?.stdout == labeled
  assert applet_run(ctx, ["-A", "-a"])?.stdout == applet_run(ctx, ["-a"])?.stdout, "the last of -a and -A wins"
  assert applet_run(ctx, ["-a", "-A"])?.stdout == labeled
}

test test_uname_errors_and_help { |ctx|
  let extra = applet_run(ctx, ["x"])?
  assert extra.status == 1
  assert extra.stderr == "uname: extra operand 'x'\nTry 'uname --help' for more information.\n", extra.stderr

  let bad = applet_run(ctx, ["--definitely-invalid"])?
  assert bad.status == 1
  assert bad.stderr == "uname: unrecognized option '--definitely-invalid'\nTry 'uname --help' for more information.\n", bad.stderr
  assert "Print certain system information." in applet_run(ctx, ["--help"])?.stdout
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("uname ")
}
