# Runs `ip` under the `linux` test fake, whose fixed interfaces and routes keep
# the output independent of the host.
proc ip_output(ctx: TestContext, argv: List[Str]) [fs, process, error] -> Str {
  test.linux_fake(ctx)?
  let source = fp"{ctx.core_dir}/ip.xsh".read_text()?
  let output = test.run_script(ctx, source, argv, {}, b"", "ip")?
  assert output.success, output.stderr
  output.stdout
}

test test_ip_addr_smoke { |ctx|
  let output = ip_output(ctx, ["addr"])
  assert "eth0" in output
}

test test_ip_route_list_patterns_accept_exact_command_forms { |ctx|
  let route = ip_output(ctx, ["route"])
  let explicit = ip_output(ctx, ["route", "show"])
  assert route == explicit
}

test test_ip_addr_alternatives_accept_address_show_and_device_forms { |ctx|
  let short = ip_output(ctx, ["addr"])
  for argv in [["address"], ["addr", "show"], ["address", "show"]] {
    assert ip_output(ctx, argv) == short
  }

  let device = ip_output(ctx, ["addr", "dev", "eth0"])
  for argv in [["address", "dev", "eth0"], ["addr", "show", "dev", "eth0"], ["address", "show", "dev", "eth0"]] {
    assert ip_output(ctx, argv) == device
  }
}
