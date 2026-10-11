##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_hostid.rs.
##! Each test names its origin; expected values are the original assertions.

use support.uu as uu

# origin: uutils test_hostid::test_help
test test_uu_hostid_help { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "hostid", ["--help"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "Print the numeric identifier")
}

# origin: uutils test_hostid::test_invalid_flag
test test_uu_hostid_invalid_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "hostid", ["--invalid-argument"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
}

# origin: uutils test_hostid::test_output_format
test test_uu_hostid_output_format { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "hostid", [])?
  uu.succeeds(r)
  assert regex.compile("^[0-9a-f]{8}\n$")?.matches(r.stdout.utf8()?)
}
