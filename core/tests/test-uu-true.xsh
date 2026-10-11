##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_true.rs.

use support.uu as uu

# origin: uutils test_true::test_no_args
test test_uu_true_no_args { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "true", [])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_true::test_version
test test_uu_true_version { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "true", ["--version"])?
  uu.succeeds(r)
  # GNU uses a two-component version and additional product notices; check the
  # version banner without imposing uutils' single-line SemVer branding.
  uu.stdout_str_starts_with(r, "true (")
  assert regex.compile("^true \\([^\n]+\\) [0-9]+\\.[0-9]+(\\.[0-9]+)?\n")?.matches(r.stdout.utf8()?)
}

# origin: uutils test_true::test_help
test test_uu_true_help { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "true", ["--help"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "true")
}

# origin: uutils test_true::test_short_options
test test_uu_true_short_options { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-h", "-V"] {
    let r = uu.invoke(s, "true", [option])?
    uu.succeeds(r)
    uu.no_output(r)
  }
}

# origin: uutils test_true::test_extra_args
test test_uu_true_extra_args { |ctx|
  let s = uu.scene(ctx)?
  for option in ["--help", "--version"] {
    let r = uu.invoke(s, "true", [option, "test"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
}

# origin: uutils test_true::test_conflict
test test_uu_true_conflict { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "true", ["--help", "--version"])?
  uu.succeeds(r)
  uu.no_output(r)
}
