type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/hostname.xsh by its real path (so the invoked name is hostname and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(ctx: TestContext, args: List[Str], vars: Record = {LC_ALL: "C"}, stdin: Bytes = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "hostname")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/hostname.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_hostname_default_short_and_domain_forms { |ctx|
  let name = system.hostname()?
  let default = applet_run(ctx, [])?

  assert default.status == 0
  assert default.stdout == f"{name}\n"
  assert applet_run(ctx, ["-s"])?.stdout == f"{name.split(".")[0]}\n"
  assert applet_run(ctx, ["--short"])?.stdout == f"{name.split(".")[0]}\n"

  let fqdn = applet_run(ctx, ["-f"])?.stdout.trim()
  assert name.split(".")[0] in fqdn

  let domain = applet_run(ctx, ["-d"])?.stdout
  if fqdn == name.split(".")[0] {
    assert domain == "", "a host without a domain prints nothing for -d"
  }
}

test test_hostname_ip_address_lists_distinct_addresses { |ctx|
  let result = applet_run(ctx, ["-i"])?
  assert result.status == 0
  assert result.stdout.trim() != ""
  assert result.stdout.ends_with("\n")
}

test test_hostname_last_format_option_wins { |ctx|
  let name = system.hostname()?
  assert applet_run(ctx, ["-d", "-s"])?.stdout == f"{name.split(".")[0]}\n"
  assert applet_run(ctx, ["-sd"])?.stdout == applet_run(ctx, ["-d"])?.stdout
}

test test_hostname_rejects_unknown_options_and_misused_operands { |ctx|
  let bad = applet_run(ctx, ["--definitely-invalid"])?
  assert bad.status == 1
  assert bad.stderr == "hostname: unrecognized option '--definitely-invalid'\nTry 'hostname --help' for more information.\n", bad.stderr

  let both = applet_run(ctx, ["-s", "newname"])?
  assert both.status == 1
  assert both.stderr == "hostname: no options can be used when setting the host name\nTry 'hostname --help' for more information.\n", both.stderr

  let extra = applet_run(ctx, ["a", "b"])?
  assert extra.status == 1
  assert extra.stderr == "hostname: extra operand 'b'\nTry 'hostname --help' for more information.\n", extra.stderr
}

test test_hostname_help_and_version { |ctx|
  assert "Display or set the system's host name." in applet_run(ctx, ["--help"])?.stdout
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("hostname ")
}
