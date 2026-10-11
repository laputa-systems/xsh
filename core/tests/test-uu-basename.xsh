##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_basename.rs.
##! Each test names its origin; expected values are the original assertions.

use support.uu as uu

# origin: uutils test_basename::test_help
test test_uu_basename_help { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["--help"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_contains(r, "Usage:")
}

# origin: uutils test_basename::test_directory
test test_uu_basename_directory { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["/root/alpha/beta/gamma/delta/epsilon/omega/"])?
  uu.succeeds(r)
  uu.stdout_only(r, "omega\n")
}

# origin: uutils test_basename::test_file
test test_uu_basename_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["/etc/passwd"])?
  uu.succeeds(r)
  uu.stdout_only(r, "passwd\n")
}
