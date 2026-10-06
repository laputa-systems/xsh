use core.lib.net_diagnostics as diagnostics

test test_nslookup_options {
  let opts = diagnostics.nslookup_options(["-type=AAAA", "-port=5353", "-timeout=2", "fixture.test", "127.0.0.1"])?
  assert opts.name == "fixture.test"
  assert opts.record == "AAAA"
  assert opts.server == "127.0.0.1"
  assert opts.port == 5353
  assert opts.timeout == 2s
}

test test_nslookup_owned_dns_server { |ctx|
  let server = env.get_or("XSH_DNS_TEST_SERVER", "")?
  if server == "" { test.skip("requires the owned loopback DNS fixture"); return }
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/nslookup.xsh" -- -type=A fixture.test $server
  assert result.status.exited_with(0), result.stderr
  assert "Name:\tfixture.test" in result.stdout
  assert "Address: 192.0.2.10" in result.stdout
}
