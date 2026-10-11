##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_printenv.rs.
##! Each test names its origin; assertions preserve the upstream observables.

use support.uu as uu

# origin: uutils test_printenv::test_get_all
test test_uu_printenv_get_all { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printenv", [], vars: {HOME: "FOO", KEY: "VALUE"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "HOME=FOO")
  uu.stdout_contains(r, "KEY=VALUE")
}

# origin: uutils test_printenv::test_get_var
test test_uu_printenv_get_var { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printenv", ["KEY"], vars: {KEY: "VALUE", FOO: "BAR"})?
  uu.succeeds(r)
  uu.stdout_is(r, "VALUE\n")
}

# An environment pair with key a=b and value c is the raw entry a=b=c;
# the first equals sign separates its native name from its value.
# origin: uutils test_printenv::test_ignore_equal_var
test test_uu_printenv_ignore_equal_var { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printenv", ["a=b"], vars: {a: "b=c"})?
  uu.fails(r)
  uu.no_stdout(r)
}

# origin: uutils test_printenv::test_silent_error_equal_var
test test_uu_printenv_silent_error_equal_var { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printenv", ["KEY", "a=b"], vars: {KEY: "VALUE", a: "b=c"})?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "VALUE\n")
  uu.no_stderr(r)
}

# origin: uutils test_printenv::test_silent_error_not_present
test test_uu_printenv_silent_error_not_present { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printenv", ["FOO", "KEY"], vars: {KEY: "VALUE"})?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "VALUE\n")
  uu.no_stderr(r)
}

# origin: uutils test_printenv::test_null_separator
test test_uu_printenv_null_separator { |ctx|
  let s = uu.scene(ctx)?
  for null_opt in ["-0", "--null"] {
    let all = uu.invoke(s, "printenv", [null_opt], vars: {HOME: "FOO", KEY: "VALUE"})?
    uu.succeeds(all)
    uu.stdout_contains(all, "HOME=FOO\0")
    uu.stdout_contains(all, "KEY=VALUE\0")
    let selected = uu.invoke(s, "printenv", [null_opt, "HOME", "KEY"],
      vars: {HOME: "FOO", KEY: "VALUE", FOO: "BAR"})?
    uu.succeeds(selected)
    uu.stdout_is_bytes(selected, b"FOO\0VALUE\0")
  }
}

# origin: uutils test_printenv::test_non_utf8_value
test test_uu_printenv_non_utf8_value { |ctx|
  let s = uu.scene(ctx)?
  let value = Path.parse_bytes(b"/tmp/lib.so\xff")?
  let r = uu.invoke(s, "printenv", ["LD_PRELOAD"], vars: {LD_PRELOAD: value})?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"/tmp/lib.so\xff\n")
}

# origin: uutils test_printenv::test_non_utf8_env_vars
test test_uu_printenv_non_utf8_env_vars { |ctx|
  let s = uu.scene(ctx)?
  let value = Path.parse_bytes(b"hello\x80world")?
  let r = uu.invoke(s, "printenv", [], vars: {NON_UTF8_VAR: value})?
  uu.succeeds(r)
  assert b"NON_UTF8_VAR=hello\x80world" in r.stdout
}
