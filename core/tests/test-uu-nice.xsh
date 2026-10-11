##! Native coverage transcribed from the uutils nice integration tests.

use support.uu as uu

# GNU diagnoses this usage error with a lowercase sentence and no final period.
# origin: uutils test_nice::test_adjustment_with_no_command_should_error
test test_uu_nice_adjustment_with_no_command_should_error { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", ["-n", "19"])?
  uu.fails(r)
  uu.stderr_only(r, "nice: a command must be given with an adjustment\nTry 'nice --help' for more information.\n")
}

# origin: uutils test_nice::test_bare_adjustment
test test_uu_nice_bare_adjustment { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", ["-1", "echo", "-n", "a"])?
  uu.succeeds(r)
  uu.stdout_is(r, "a")
}

# origin: uutils test_nice::test_command_where_command_takes_n_flag
test test_uu_nice_command_where_command_takes_n_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", ["-n", "19", "echo", "-n", "a"])?
  uu.succeeds(r)
  uu.stdout_is(r, "a")
}

# origin: uutils test_nice::test_command_with_args
test test_uu_nice_command_with_args { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", ["-n", "19", "echo", "a", "b", "c"])?
  uu.succeeds(r)
  uu.stdout_is(r, "a b c\n")
}

# origin: uutils test_nice::test_command_with_no_adjustment
test test_uu_nice_command_with_no_adjustment { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", ["echo", "a"])?
  uu.succeeds(r)
  uu.stdout_is(r, "a\n")
}

# origin: uutils test_nice::test_command_with_no_args
test test_uu_nice_command_with_no_args { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", ["-n", "19", "echo"])?
  uu.succeeds(r)
  uu.stdout_is(r, "\n")
}

# origin: uutils test_nice::test_get_current_niceness
test test_uu_nice_get_current_niceness { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", [])?
  uu.succeeds(r)
  uu.stdout_is(r, f"{process.priority()?}\n")
}

# origin: uutils test_nice::test_invalid_argument
test test_uu_nice_invalid_argument { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", ["--invalid"])?
  uu.fails_with_code(r, 125)
}

# An unprivileged process cannot lower its niceness; the command still runs.
# GNU reports the refused adjustment as "cannot set niceness".
# origin: uutils test_nice::test_nice_adj_negative
test test_uu_nice_nice_adj_negative { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", ["--adj", "-20", "true"])?
  uu.succeeds(r)
  uu.stderr_is(r, "nice: cannot set niceness: Permission denied\n")
}

# origin: uutils test_nice::test_nice_huge
test test_uu_nice_nice_huge { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", ["-n", "99999999999999999999999999999999999999999999999999999999999999999999999999999999999999999", "true"])?
  uu.succeeds(r)
  uu.no_stdout(r)
}

# origin: uutils test_nice::test_nice_huge_negative
test test_uu_nice_nice_huge_negative { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", ["-n", "-9999999999", "true"])?
  uu.succeeds(r)
}

# origin: uutils test_nice::test_sign_middle
test test_uu_nice_sign_middle { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", ["-n", "-2+4", "true"])?
  uu.fails_with_code(r, 125)
  uu.no_stdout(r)
  uu.stderr_contains(r, "invalid")
}

# GNU uses the short-option diagnostic for the trailing argumentless -n.
# origin: uutils test_nice::test_trailing_empty_adjustment
test test_uu_nice_trailing_empty_adjustment { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nice", ["-n", "1", "-n"])?
  uu.fails(r)
  assert r.stderr.utf8()?.starts_with("nice: option requires an argument -- 'n'")
}

