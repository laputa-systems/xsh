use core.lib.net_diagnostics as diagnostics

test test_dig_options {
  let opts = diagnostics.dig_options(["@127.0.0.1", "-p", "5353", "+short", "+time=1", "+tries=2", "fixture.test", "AAAA"])?
  assert opts.name == "fixture.test"
  assert opts.record == "AAAA"
  assert opts.server == "127.0.0.1"
  assert opts.port == 5353
  assert opts.short
  assert opts.timeout == 1s
  assert opts.attempts == 2
  assert diagnostics.dig_options(["+trace", "fixture.test"]) is Err(_)
}

test test_dig_answer_presentation {
  let answer = {name: "fixture.test", record: "A", value: "192.0.2.10", ttl: 60}
  assert diagnostics.answer_text(answer, true) == "192.0.2.10\n"
  assert diagnostics.answer_text(answer, false) == "fixture.test.\t60\tIN\tA\t192.0.2.10\n"
}

test test_dig_owned_dns_server { |ctx|
  let server = env.get_or("XSH_DNS_TEST_SERVER", "")?
  if server == "" { test.skip("requires the owned loopback DNS fixture"); return }
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/dig.xsh" -- f"@{server}" +short fixture.test A
  assert result.status.exited_with(0), result.stderr
  assert result.stdout == "192.0.2.10\n"
}
