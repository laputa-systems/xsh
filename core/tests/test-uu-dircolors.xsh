##! Transcribed from the uutils dircolors integration tests.

use support.uu as uu

proc fixture_output(s: uu.Scene, r: uu.Ran, name: Str) [fs, error] {
  uu.remove(s, "expected")?
  uu.fixture(s, "dircolors", name, "expected")?
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dircolors::test1
test test_uu_dircolors_test1 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dircolors", "test1.txt", "test1.txt")?
  for item in [{flag: "-c", suffix: "csh"}, {flag: "-b", suffix: "sh"}] {
    let flag = item.flag
    let suffix = item.suffix
    let r = uu.invoke(s, "dircolors", [flag, "test1.txt"], vars: {TERM: "gnome"})?
    uu.succeeds(r)
    fixture_output(s, r, f"test1.{suffix}.expected")?
  }
}

# origin: uutils test_dircolors::test_keywords
test test_uu_dircolors_keywords { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dircolors", "keywords.txt", "keywords.txt")?
  for item in [{flag: "-c", suffix: "csh"}, {flag: "-b", suffix: "sh"}] {
    let flag = item.flag
    let suffix = item.suffix
    let r = uu.invoke(s, "dircolors", [flag, "keywords.txt"], vars: {TERM: ""})?
    uu.succeeds(r)
    fixture_output(s, r, f"keywords.{suffix}.expected")?
  }
}

# origin: uutils test_dircolors::test_internal_db
test test_uu_dircolors_internal_db { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dircolors", ["-p"])?
  uu.succeeds(r)
  fixture_output(s, r, "internal.expected")?
}

# origin: uutils test_dircolors::test_ls_colors
test test_uu_dircolors_ls_colors { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dircolors", ["--print-ls-colors"])?
  uu.succeeds(r)
  fixture_output(s, r, "ls_colors.expected")?
}

# origin: uutils test_dircolors::test_bash_default
test test_uu_dircolors_bash_default { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dircolors", ["-b"], vars: {TERM: "screen"})?
  uu.succeeds(r)
  fixture_output(s, r, "bash_def.expected")?
}

# origin: uutils test_dircolors::test_csh_default
test test_uu_dircolors_csh_default { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dircolors", ["-c"], vars: {TERM: "screen"})?
  uu.succeeds(r)
  fixture_output(s, r, "csh_def.expected")?
}

# origin: uutils test_dircolors::test_overridable_args
test test_uu_dircolors_overridable_args { |ctx|
  let s = uu.scene(ctx)?
  for item in [{flag: "-bc", name: "csh_def.expected"}, {flag: "-cb", name: "bash_def.expected"}] {
    let flag = item.flag
    let name = item.name
    let r = uu.invoke(s, "dircolors", [flag], vars: {TERM: "screen"})?
    uu.succeeds(r)
    fixture_output(s, r, name)?
  }
}

# origin: uutils test_dircolors::test_invalid_arg
test test_uu_dircolors_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "dircolors", ["--definitely-invalid"])?, 1)
}

# origin: uutils test_dircolors::test_no_env
test test_uu_dircolors_no_env { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dircolors", [])?
  uu.fails(r)
  uu.stderr_only(r, "dircolors: no SHELL environment variable, and no shell type option given\n")
}

# origin: uutils test_dircolors::test_exclusive_option
test test_uu_dircolors_exclusive_option { |ctx|
  let s = uu.scene(ctx)?
  for args in [["-bp"], ["-cp"], ["-b", "--print-ls-colors"], ["-c", "--print-ls-colors"], ["-p", "--print-ls-colors"]] {
    let r = uu.invoke(s, "dircolors", args)?
    uu.fails(r)
    uu.stderr_contains(r, "mutually exclusive")
  }
}

# origin: uutils test_dircolors::test_stdin
test test_uu_dircolors_stdin { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dircolors", ["-b", "-"], b"owt 40;33\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "LS_COLORS='tw=40;33:';\nexport LS_COLORS\n")
}

# origin: uutils test_dircolors::test_quoting
test test_uu_dircolors_quoting { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dircolors", ["-b", "-"], b"exec 'echo Hello;:'\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "LS_COLORS='ex='\\''echo Hello;\\:'\\'':';\nexport LS_COLORS\n")
}

# origin: uutils test_dircolors::test_extra_operand
test test_uu_dircolors_extra_operand { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dircolors", ["-c", "file1", "file2"])?
  uu.fails(r)
  uu.stderr_contains(r, "dircolors: extra operand 'file2'\n")
  uu.no_stdout(r)
}

# origin: uutils test_dircolors::test_term_matching
test test_uu_dircolors_term_matching { |ctx|
  let s = uu.scene(ctx)?
  for item in [{pattern: "matches", term: "matches", matched: true}, {pattern: "matches", term: "no_match", matched: false}, {pattern: "[!a]_negation", term: "a_negation", matched: false}, {pattern: "[!a]_negation", term: "b_negation", matched: true}, {pattern: "[^a]_negation", term: "a_negation", matched: false}, {pattern: "[^a]_negation", term: "b_negation", matched: true}] {
    let pattern = item.pattern
    let term = item.term
    let matched = item.matched
    let theme = f"\nTERM {pattern}\n\n.term_matching    00;38;5;61\n"
    let r = uu.invoke(s, "dircolors", ["-b", "-"], bytes.from_text(theme), vars: {TERM: term})?
    uu.succeeds(r)
    uu.stdout_only(r, if matched { "LS_COLORS='*.term_matching=00;38;5;61:';\nexport LS_COLORS\n" } else { "LS_COLORS='';\nexport LS_COLORS\n" })
  }
}

# origin: uutils test_dircolors::test_dircolors_for_dir_as_file
test test_uu_dircolors_dircolors_for_dir_as_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dircolors", ["-c", "/"])?
  uu.fails(r)
  uu.no_stdout(r)
  assert r.stderr.utf8()?.trim() == "dircolors: /: read error: Is a directory"
}

# origin: uutils test_dircolors::test_repeated
test test_uu_dircolors_repeated { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["-b", "-c", "--print-database", "--print-ls-colors"] {
    let r = uu.invoke(s, "dircolors", [arg, arg])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
}

# origin: uutils test_dircolors::test_colorterm_empty_with_wildcard
test test_uu_dircolors_colorterm_empty_with_wildcard { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dircolors", ["-b", "-"], b"COLORTERM ?*\nowt 40;33\n", vars: {COLORTERM: ""})?
  uu.succeeds(r)
  uu.stdout_only(r, "LS_COLORS='';\nexport LS_COLORS\n")
}

# origin: uutils test_dircolors::test_invalid_term_glob
test test_uu_dircolors_invalid_term_glob { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dircolors", ["-b", "-"], b"TERM [\nDIR 01;34\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "LS_COLORS='';\nexport LS_COLORS\n")
}

# origin: uutils test_dircolors::test_dircolors_non_utf8_paths
test test_uu_dircolors_dircolors_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let filename = uu.at_bytes(s, b"\xff\xfe")?
  filename.write(b"NORMAL 00\n*.txt 32\n")?
  let r = uu.invoke_paths(s, "dircolors", [Path.parse_bytes(b"\xff\xfe")?], vars: {SHELL: "bash"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "LS_COLORS=")
  uu.stdout_contains(r, "*.txt=32")
}
