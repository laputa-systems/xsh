const TOOL = r"""##! A repository tool.

proc default_repo() [io] -> Str {
  print "DEFAULT-EVALUATED"
  "/srv/repo"
}

print "TOP-LEVEL"

## Check a repository.
##
## Detail that the listing leaves out.
cli main repo check(repo: Str = default_repo(), deep_scan: Bool = false) [io] {
  print f"check {repo} {deep_scan}"
}

## Synchronize a repository with its mirrors.
cli main repo sync_all(target: Str, jobs: Int = 2) {
  print f"sync {target} {jobs}"
}

cli main status(...names: List[Str]) {
  print f"status {names.join(",")}"
}
"""

test test_subcommand_path_selects_the_entry_and_its_parser { |ctx|
  let checked = test.expect(ctx, TOOL, status: 0, args: ["repo", "check", "--repo", "/x", "--deep-scan"])?
  assert checked.stdout == "TOP-LEVEL\ncheck /x true\n"
  # A path word is typed in kebab case, as an option is.
  let synced = test.expect(ctx, TOOL, status: 0, args: ["repo", "sync-all", "mirror", "--jobs=5"])?
  assert synced.stdout == "TOP-LEVEL\nsync mirror 5\n"
  let status = test.expect(ctx, TOOL, status: 0, args: ["status", "a", "b"])?
  assert status.stdout == "TOP-LEVEL\nstatus a,b\n"
}

test test_subcommand_help_lists_each_group_without_running_the_script { |ctx|
  let top = test.expect(
    ctx,
    TOOL,
    status: 0,
    stdout: [
      "A repository tool.\n\nusage: ",
      " COMMAND [ARGS]...\n\ncommands:\n",
      "  repo check     Check a repository.\n",
      "  repo sync-all  Synchronize a repository with its mirrors.\n",
      "  status\n",
    ],
    args: ["--help"],
  )?
  assert "TOP-LEVEL" not in top.stdout, top.stdout
  assert "Detail" not in top.stdout, top.stdout
  let listing = test.expect(
    ctx,
    TOOL,
    status: 0,
    stdout: [" repo COMMAND [ARGS]...\n", "  check     Check a repository.\n", "  sync-all  "],
    args: ["repo", "-h"],
  )?
  assert "status" not in listing.stdout, listing.stdout
  assert "A repository tool." not in listing.stdout, listing.stdout
  # An entry's help is its own: its whole doc comment, and a computed default
  # as it is written, not evaluated.
  let entry = test.expect(
    ctx,
    TOOL,
    status: 0,
    stdout: [
      "Check a repository.\n\nDetail that the listing leaves out.\n\nusage: ",
      " repo check [OPTIONS]\n",
      "--repo REPO  Str, default: default_repo()",
      "--deep-scan  Bool, default: false",
    ],
    args: ["repo", "check", "--help"],
  )?
  assert "DEFAULT-EVALUATED" not in entry.stdout, entry.stdout
  assert "TOP-LEVEL" not in entry.stdout, entry.stdout
}

test test_missing_or_unknown_subcommand_is_a_usage_error { |ctx|
  for {arguments, message, usage} in [
    {arguments: [], message: "missing subcommand", usage: " COMMAND [ARGS]..."},
    {arguments: ["repo"], message: "missing subcommand", usage: " repo COMMAND [ARGS]..."},
    {arguments: ["bogus"], message: "unknown subcommand `bogus`", usage: " COMMAND [ARGS]..."},
    {arguments: ["repo", "sync_all", "x"], message: "unknown subcommand `sync_all`", usage: "  sync-all  "},
    {arguments: ["repo", "sync-all"], message: "TARGET", usage: " repo sync-all TARGET [OPTIONS]"},
    {arguments: ["repo", "check", "--bogus"], message: "--bogus", usage: " repo check [OPTIONS]"},
  ] {
    let rejected = test.expect(ctx, TOOL, status: 2, stderr: [message, usage], args: arguments)?
    assert rejected.stdout == "", rejected.stdout
  }
}

test test_computed_default_runs_after_parsing_only_for_an_omitted_option { |ctx|
  let omitted = test.expect(ctx, TOOL, status: 0, args: ["repo", "check"])?
  assert omitted.stdout == "TOP-LEVEL\nDEFAULT-EVALUATED\ncheck /srv/repo false\n"
  let given = test.expect(ctx, TOOL, status: 0, args: ["repo", "check", "--repo=/x"])?
  assert "DEFAULT-EVALUATED" not in given.stdout, given.stdout
  # A bare entry computes defaults the same way; each omitted option is
  # evaluated in declaration order and a given one between them is kept.
  let bare = r"""
proc stamp(name: Str, value: Int) [io] -> Int {
  print f"default {name}"
  value
}

cli main(input: Str, first: Int = stamp("first", 1), second: Int = 2, third: Int = stamp("third", 3)) [io] {
  print f"{input} {first} {second} {third}"
}
"""
  let all = test.expect(ctx, bare, status: 0, args: ["in"])?
  assert all.stdout == "default first\ndefault third\nin 1 2 3\n"
  let some = test.expect(ctx, bare, status: 0, args: ["in", "--third", "30", "--second=20"])?
  assert some.stdout == "default first\nin 1 20 30\n"
  let help = test.expect(
    ctx,
    bare,
    status: 0,
    stdout: ["--first FIRST  Int, default: stamp(\"first\", 1)", "--second SECOND  Int, default: 2"],
    args: ["--help"],
  )?
  assert "default first" not in help.stdout, help.stdout
}

test test_computed_default_failure_is_a_script_failure_before_the_body { |ctx|
  let failed = test.expect(
    ctx,
    r"""
proc required_root() [error] -> Result[Str] {
  fail "ROOT is not configured"
}

print "TOP-LEVEL"

cli main build(root: Str = required_root()?) [error, io] {
  print f"BODY {root}"
}
""",
    status: 3,
    stderr: ["ROOT is not configured"],
    args: ["build"],
  )?
  assert failed.stdout == "TOP-LEVEL\n", failed.stdout
}

test test_subcommand_declarations_are_checked { |ctx|
  for {source, message} in [
    {
      source: """cli main build() [] {}
cli main() [] {}
""",
      message: "a bare `cli main` cannot be declared beside another CLI entry",
    },
    {
      source: """cli main() [] {}
cli main() [] {}
""",
      message: "a bare `cli main` cannot be declared beside another CLI entry",
    },
    {
      source: """cli main sync_all() [] {}
cli main sync_all() [] {}
""",
      message: "this subcommand path is already declared",
    },
    {
      source: """cli main repo(name: Str) [] { let _ = name }
cli main repo check() [] {}
""",
      message: "a subcommand path cannot continue another entry's path",
    },
    {
      source: "cli main help() [] {}\n",
      message: "`help` is reserved by the CLI parser",
    },
    {
      source: """proc default_jobs() [io] -> Int { print "DEFAULT-MARKER"; 4 }
cli main build(jobs: Int = default_jobs()) [] { let _ = jobs }
""",
      message: "effect `io` required by `default_jobs` is not in caller's declared effects",
    },
    {
      source: "cli main build(root: Path = 4) [] { let _ = root }\n",
      message: "check.type-mismatch",
    },
  ] {
    let rejected = test.expect(ctx, source, status: 2, stderr: [message], args: ["--help"])?
    assert "MARKER" not in rejected.stdout, rejected.stdout
  }
}

test test_subcommand_entry_in_a_module_is_rejected { |ctx|
  let root = test.temp_dir(ctx, name: "cli-module-entry")?
  fp"{root}/commands.xsh".write(
    """##! Commands that belong in the entry script.
cli main build() [] {}
""",
  )
  let script = fp"{root}/tool.xsh"
  script.write("use commands\n")
  let output = run.capture --text ${ctx.xsh_bin} $script build
  assert output.status.exited_with(2), output.stderr
  assert "check.cli-entry" in output.stderr, output.stderr
  assert "`cli main` is only permitted in the entry module" in output.stderr, output.stderr
}

test test_subcommand_entries_format_and_lint_as_declared { |ctx|
  let root = test.temp_dir(ctx, name: "cli-subcommand-tooling")?
  let script = fp"{root}/tool.xsh"
  script.write(TOOL)
  let formatted = run.capture --text "xsht" fmt --check $script
  assert formatted.status.exited_with(0), f"{formatted.stdout}{formatted.stderr}"
  let linted = run.capture --text "xsht" lint --only lint.unused-callable,lint.unused-local $script
  assert linted.status.exited_with(0), linted.stderr
  assert "warn[" not in linted.stderr, linted.stderr
}
