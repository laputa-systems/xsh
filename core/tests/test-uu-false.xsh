##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_false.rs.

use support.uu as uu

# origin: uutils test_false::test_no_args
test test_uu_false_no_args { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "false", [])?
  uu.fails(r)
  uu.no_output(r)
}

# origin: uutils test_false::test_version
test test_uu_false_version { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "false", ["--version"])?
  uu.fails(r)
  # GNU uses a two-component version and additional product notices; check the
  # version banner without imposing uutils' single-line SemVer branding.
  uu.stdout_str_starts_with(r, "false (")
  assert regex.compile("^false \\([^\n]+\\) [0-9]+\\.[0-9]+(\\.[0-9]+)?\n")?.matches(r.stdout.utf8()?)
}

# origin: uutils test_false::test_help
test test_uu_false_help { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "false", ["--help"])?
  uu.fails(r)
  uu.stdout_contains(r, "false")
}

# origin: uutils test_false::test_short_options
test test_uu_false_short_options { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-h", "-V"] {
    let r = uu.invoke(s, "false", [option])?
    uu.fails(r)
    uu.no_output(r)
  }
}

# origin: uutils test_false::test_extra_args
test test_uu_false_extra_args { |ctx|
  let s = uu.scene(ctx)?
  for option in ["--help", "--version"] {
    let r = uu.invoke(s, "false", [option, "test"])?
    uu.fails(r)
    uu.no_output(r)
  }
}

# origin: uutils test_false::test_conflict
test test_uu_false_conflict { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "false", ["--help", "--version"])?
  uu.fails(r)
  uu.no_output(r)
}
