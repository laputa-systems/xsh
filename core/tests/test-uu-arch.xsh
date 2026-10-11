##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_arch.rs.
##! Each test names its origin; expected values are the original assertions.

use support.uu as uu

# origin: uutils test_arch::test_arch
test test_uu_arch_arch { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "arch", [])?
  uu.succeeds(r)
  uu.stdout_contains(r, "\n")
}

# origin: uutils test_arch::test_arch_output_is_not_empty
test test_uu_arch_arch_output_is_not_empty { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "arch", [])?
  uu.succeeds(r)
  assert !r.stdout.utf8()?.trim().is_empty(), "arch output was empty"
}

# origin: uutils test_arch::test_invalid_arg
test test_uu_arch_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "arch", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}
