##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_hostname.rs.
##! Each test names its origin; expected values are the original assertions.

use support.uu as uu

# origin: uutils test_hostname::test_hostname
test test_uu_hostname_hostname { |ctx|
  let s = uu.scene(ctx)?
  let default = uu.invoke(s, "hostname", [])?
  uu.succeeds(default)
  let short = uu.invoke(s, "hostname", ["-s"])?
  uu.succeeds(short)
  let domain = uu.invoke(s, "hostname", ["-d"])?
  uu.succeeds(domain)

  assert default.stdout.len() >= short.stdout.len()
  assert default.stdout.len() >= domain.stdout.len()
}

# origin: uutils test_hostname::test_hostname_domain_empty
test test_uu_hostname_hostname_domain_empty { |ctx|
  let s = uu.scene(ctx)?
  let fqdn = uu.invoke(s, "hostname", ["-f"])?
  uu.succeeds(fqdn)
  let short = uu.invoke(s, "hostname", ["-s"])?
  uu.succeeds(short)
  let domain = uu.invoke(s, "hostname", ["-d"])?
  uu.succeeds(domain)
  let domain_short = uu.invoke(s, "hostname", ["-sd"])?
  uu.succeeds(domain_short)

  if fqdn.stdout == short.stdout {
    uu.no_stdout(domain)
    uu.no_stdout(domain_short)
  }
}

# origin: uutils test_hostname::test_hostname_full
test test_uu_hostname_hostname_full { |ctx|
  let s = uu.scene(ctx)?
  let short = uu.invoke(s, "hostname", ["-s"])?
  uu.succeeds(short)
  let name = short.stdout.utf8()?.trim()
  assert ! name.is_empty()

  let full = uu.invoke(s, "hostname", ["-f"])?
  uu.succeeds(full)
  uu.stdout_contains(full, name)
}

# origin: uutils test_hostname::test_hostname_ip
test test_uu_hostname_hostname_ip { |ctx|
  let s = uu.scene(ctx)?
  let result = uu.invoke(s, "hostname", ["-i"])?
  uu.succeeds(result)
  assert ! result.stdout.utf8()?.trim().is_empty()
}

# origin: uutils test_hostname::test_invalid_arg
test test_uu_hostname_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let result = uu.invoke(s, "hostname", ["--definitely-invalid"])?
  uu.fails_with_code(result, 1)
}
