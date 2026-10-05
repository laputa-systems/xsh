type Captured = {status: Status, stdout: Str, stderr: Str}

# A module whose first doc comment is attached to no export, which draws
# `check.orphan-doc-comment` once however many scripts import it.
const orphan_doc_helper = "##! Helper module.\n## This comment is not attached to an export.\nlet value = 1\n\n## Exports a value.\nexport let exported: Int = value\n"

const unused_local = """proc main() {
  let unused = 1
}

main()?
"""

# Writes each `files` entry, a path below the project root and its text, into
# a fresh directory.
proc project(ctx: TestContext, files: Map[Str, Str]) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "project")?
  for name in files.keys() {
    let file = fp"{root}/{name}"
    file.parent().mkdir()
    file.write(files[name])
  }

  Ok(root)
}

# Runs `xsht` with `arguments` in `root`.
proc xsht(root: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  cd (root) {
    run.capture --text "xsht" @arguments
  }
}

# What `xsht lint` printed on standard error before its closing timing line.
pure diagnostics(stderr: Str) -> Str {
  let found = rx"(?s)^(.*)xsht [a-z]+: [0-9]+ files? in [^\n]* \(thread time by stage: [^\n]*\n$".captures(stderr)
  if found.len() == 2 { found[1] } else { f"no timing line: {stderr}" }
}

# The codes of the warnings in `stderr`, in the order they were reported.
pure warning_codes(stderr: Str) -> List[Str] {
  var codes = []
  for line in stderr.lines() {
    let found = rx"^warn\[([^\]]+)\]".captures(line)
    if found.len() == 2 {
      codes += [found[1]]
    }
  }

  codes
}

test test_lint_accepts_documented_path_constructor_warning { |ctx|
  let file = test.temp_file(
    ctx,
    name: "path-constructor.xsh",
    contents: bytes.from_text("let root = Path(args[0])\nprint $root\n"),
  )?
  let linted = run.capture --text "xsht" lint $file
  assert linted.status.exited_with(0), linted.stderr
  assert linted.stdout == ""
  assert "warn[lint.path-constructor]" in linted.stderr, linted.stderr
}

test test_lint_reports_warnings_with_spans { |ctx|
  let file = test.temp_file(
    ctx,
    name: "warnings.xsh",
    contents: bytes.from_text(r"""proc main(args: List[Str]) {
  let input = args[0]
  let src = "tmp"
  let root = Path("target/lint")
  let unused = 1
  let p = Path(src)
  fs.mkdir(fp"{root}/src/lib", parents: true)?
  run grep (input) haystack ?
  if true {
    let src = "other"
    print ${src} ${args[0]}
  }
}

main(args)?
"""),
  )?
  let linted = run.capture --text "xsht" lint $file
  assert linted.status.exited_with(1), linted.stderr
  assert linted.stdout == ""
  for code in ["lint.unused-local", "lint.path-constructor", "lint.command-value", "lint.redundant-default"] {
    assert f"warn[{code}]" in linted.stderr, linted.stderr
  }

  assert file.display() in linted.stderr, linted.stderr
}

test test_lint_reports_discovered_files_in_stable_order { |ctx|
  let root = project(ctx, {"b.xsh": unused_local, "a.xsh": unused_local})?
  let linted = xsht(root, ["lint"])?
  assert linted.status.exited_with(1), linted.stderr
  let around_first = linted.stderr.split("a.xsh")
  assert around_first.len() > 1, linted.stderr
  assert "b.xsh" not in around_first[0], linted.stderr
  assert "b.xsh" in linted.stderr, linted.stderr
  assert linted.stdout == ""
}

test test_lint_mixed_parse_and_lint_failures_exit_with_parse_status { |ctx|
  let root = project(ctx, {"a.xsh": "let =\n", "b.xsh": unused_local})?
  let linted = xsht(root, ["lint"])?
  assert linted.status.exited_with(2), linted.stderr
  assert "err[parse." in linted.stderr, linted.stderr
  assert "warn[lint.unused-local]" in linted.stderr, linted.stderr
}

test test_lint_reports_check_errors_with_spans { |ctx|
  let file = test.temp_file(ctx, name: "reassign-let.xsh", contents: bytes.from_text("let x = 1\nx = 2\n"))?
  let linted = run.capture --text "xsht" lint $file
  assert linted.status.exited_with(2), linted.stderr
  assert linted.stdout == ""
  assert "err[check.assign-let]" in linted.stderr, linted.stderr
  assert file.display() in linted.stderr, linted.stderr
}

test test_lint_reports_imported_check_errors_once { |ctx|
  let root = project(
    ctx,
    {
      "lib/shared.xsh": "export pure bad() -> Int {\n  return \"not an int\"\n}\n",
      "first.xsh": "use lib.shared\nlet one = shared.bad()\n",
      "second.xsh": "use lib.shared\nlet two = shared.bad()\n",
    },
  )?
  let linted = run.capture --text "xsht" lint fp"{root}/first.xsh" fp"{root}/second.xsh"
  assert linted.status.exited_with(2), linted.stderr
  assert linted.stderr.split("err[check.type-mismatch]").len() == 2, linted.stderr
  assert fp"{root}/lib/shared.xsh".display() in linted.stderr, linted.stderr
}

test test_lint_accepts_current_syntax_and_ignores_strings_and_comments { |ctx|
  let file = test.temp_file(
    ctx,
    name: "current-syntax.xsh",
    contents: bytes.from_text(r"""# old examples: fmt"x" glob"*.rs" run.capture --text echo $name run (target)
let label = f"hello"
let files = g"*.rs"
const shell = "printf '$HOME' run.capture (target)"
const target = p"target/debug/tool"
let opts = {tool: target}
run.status $target --flag $opts.tool
print ${label}
"""),
  )?
  let linted = run.capture --text "xsht" lint $file
  assert linted.status.exited_with(0), linted.stderr
  assert linted.stdout == ""
  assert diagnostics(linted.stderr) == "", linted.stderr
}

test test_lint_accepts_a_cli_main_entry_beside_other_entry_files { |ctx|
  let root = project(
    ctx,
    {"tool.xsh": "cli main(name: Str) {\n  print $name\n}\n", "other.xsh": "print \"other\"\n"},
  )?
  let linted = xsht(root, ["lint", "."])?
  assert linted.status.exited_with(0), linted.stderr
}

test test_lint_list_jsonl_names_every_selectable_code_with_a_summary {
  let listed = run.capture --text "xsht" lint --list --format jsonl
  assert listed.status.exited_with(0), listed.stderr
  let lines = listed.stdout.lines().collect()
  assert lines.len() > 50, listed.stdout
  for line in lines {
    assert line.starts_with("{\"code\":\"") and "\",\"summary\":\"" in line, line
  }

  assert "{\"code\":\"lint.prefer-guard\",\"summary\":" in listed.stdout, listed.stdout
  assert "{\"code\":\"check.bool-statement\",\"summary\":" in listed.stdout, listed.stdout

  let rejected = run.capture --text "xsht" lint --list --fix
  assert rejected.status.exited_with(2), rejected.stderr
}

test test_lint_explicit_directory_lints_xsh_files { |ctx|
  let root = project(ctx, {"project/main.xsh": "const value = 1\n", "project/nested/helper.xsh": "const value = 2\n"})?
  let linted = xsht(root, ["lint", fp"{root}/project".display()])?
  assert linted.status.exited_with(0), linted.stderr
}

# A file found by discovery from a directory above its project is governed by
# the project's config: the config's `exclude` drops it and its `module_path`
# resolves its imports, exactly as when the command starts in the project.
test test_lint_discovery_applies_the_nearest_config_of_each_file { |ctx|
  let root = project(
    ctx,
    {
      "project/xsht-config.ini": "exclude = ignored/**\nmodule_path = lib\n",
      "project/lib/helper.xsh": "##! Nested config helper module.\n## Returns the configured helper value.\nexport pure value() -> Str {\n  \"ok\"\n}\n",
      "project/app/main.xsh": "use helper\nprint helper.value()\n",
      "project/ignored/bad.xsh": "let =\n",
    },
  )?
  let linted = xsht(root, ["lint"])?
  assert linted.status.exited_with(0), linted.stderr
  assert linted.stdout == ""
  assert diagnostics(linted.stderr) == ""
}

test test_lint_fix_deduplicates_diagnostics_from_imported_modules { |ctx|
  let entry = "use helper\nprint helper.exported\n"
  let root = project(ctx, {"helper.xsh": orphan_doc_helper, "first.xsh": entry, "second.xsh": entry})?
  let fixed = xsht(root, ["lint", "--fix", "first.xsh", "second.xsh"])?
  assert fixed.status.exited_with(2), fixed.stderr
  assert fixed.stderr.split("check.orphan-doc-comment").len() == 2, fixed.stderr
}

test test_lint_directory_uses_import_graph_roots { |ctx|
  let root = project(ctx, {"helper.xsh": orphan_doc_helper, "main.xsh": "use helper\nprint helper.exported\n"})?
  let linted = xsht(root, ["lint", "."])?
  assert linted.status.exited_with(2), linted.stderr
  # The imported module is linted through its entry root, once.
  assert linted.stderr.split("check.orphan-doc-comment").len() == 2, linted.stderr
}

test test_lint_directory_cycle_selects_one_component_root { |ctx|
  let root = project(
    ctx,
    {
      "a.xsh": "use b\n##! A module.\n## This comment is not attached to an export.\nlet a = 1\n",
      "b.xsh": "use a\n##! B module.\n## This comment is not attached to an export.\nlet b = 1\n",
    },
  )?
  let linted = xsht(root, ["lint", "."])?
  assert linted.status.exited_with(2), linted.stderr
  # The cycle is reported once, through the one root selected for it.
  assert linted.stderr.split("parse.module-cycle").len() == 2, linted.stderr
}

test test_lint_only_restricts_diagnostics_and_fixes_to_named_codes { |ctx|
  let root = project(ctx, {"main.xsh": "let name: Str = \"x\"\nprint \${name.byte_len()}\n"})?
  let all = xsht(root, ["lint", "main.xsh"])?
  assert warning_codes(all.stderr) == [
    "lint.prefer-const",
    "lint.needless-annotation",
    "lint.redundant-command-interpolation",
  ]

  let selected = xsht(
    root,
    ["lint", "--only", "lint.needless-annotation,lint.redundant-command-interpolation", "main.xsh"],
  )?
  assert selected.status.exited_with(1), selected.stderr
  assert warning_codes(selected.stderr) == ["lint.needless-annotation", "lint.redundant-command-interpolation"]

  let fixed = xsht(root, ["lint", "--only=lint.needless-annotation", "--fix", "main.xsh"])?
  assert fixed.status.exited_with(0), fixed.stderr
  assert fp"{root}/main.xsh".read_text()? == "let name = \"x\"\nprint \${name.byte_len()}\n"
  let remaining = xsht(root, ["lint", "main.xsh"])?
  assert warning_codes(remaining.stderr) == ["lint.prefer-const", "lint.redundant-command-interpolation"]
}

test test_lint_fix_applies_the_bool_statement_assert_fix { |ctx|
  let source = r"""let xs = [1, 2]
xs == [1, 2] # comment stays
proc note(value: Int) -> Int {
    print "eval ${value}"
    value
}
proc check(n: Int) {
    match n {
        1 => note(0) < note(n),
        _ => {},
    }
    note(0) < note(n) < note(3)
}
check(1)?
check(5)?
"""
  let root = project(ctx, {"main.xsh": source})?
  let listed = xsht(root, ["lint", "main.xsh"])?
  assert listed.status.exited_with(2), listed.stderr
  assert listed.stderr.split("err[check.bool-statement]").len() == 4, listed.stderr
  assert "help: insert `assert` in a braced match arm -> { assert note(0) < note(n) }" in listed.stderr, listed.stderr

  let fixed = xsht(root, ["lint", "--fix", "main.xsh"])?
  assert fixed.status.exited_with(0), fixed.stderr
  let migrated = fp"{root}/main.xsh".read_text()?
  for statement in [
    "assert xs == [1, 2] # comment stays",
    "1 => assert note(0) < note(n)",
    "  assert note(0) < note(n) < note(3)",
  ] {
    assert statement in migrated, migrated
  }

  let again = xsht(root, ["lint", "--fix", "main.xsh"])?
  assert again.status.exited_with(0), again.stderr
  # A second fix makes no change.
  assert fp"{root}/main.xsh".read_text()? == migrated

  let traced = xsht(root, ["trace", "main.xsh"])?
  assert traced.status.exited_with(3), traced.stderr
  # Operands evaluate once, in order.
  assert traced.stdout.starts_with("eval 0\neval 1\neval 0\neval 1\neval 3\neval 0\neval 5\neval 3\n"), traced.stdout
  assert "assertion failed: note(0) < note(n) < note(3)\nordering comparison failed: 5 < 3" in traced.stderr, traced.stderr
}

test test_lint_only_bool_statement_applies_only_its_assert_fix { |ctx|
  let root = project(ctx, {"main.xsh": "let name: Str = \"x\"\nname == \"x\"\n"})?
  let fixed = xsht(root, ["lint", "--only", "check.bool-statement", "--fix", "main.xsh"])?
  assert fixed.status.exited_with(0), fixed.stderr
  assert fp"{root}/main.xsh".read_text()? == "let name: Str = \"x\"\nassert name == \"x\"\n"
}

# The discard fix prefixes the statement wherever it sits, including in an
# imported module checked as part of its importer, and leaves the
# diagnostics it cannot repair mechanically in place.
test test_lint_only_ignored_result_discards_values_with_let { |ctx|
  let root = project(
    ctx,
    {
      "main.xsh": "use helper\nproc data() [io] -> Int { print data; 1 }\ndata()\nif true { data() }\nmatch 1 {\n  1 => data(),\n  _ => {}\n}\nhelper.go()?\nprint done\n",
      "helper.xsh": "##! Helper module.\nproc data() [io] -> Int { print helper; 2 }\n## Run the helper.\nexport proc go() [io] {\n  data()\n  print go\n}\n",
      "rejected.xsh": "var items = [1]\nitems.push(2)\nproc parse() [error] -> Result[Int] { 1 }\nparse()\nprint \${items.len()}\n",
    },
  )?

  # Each file is fixed by its own lint node; the importer's node may still
  # report the module's diagnostic from before that node ran, so the exit
  # status of the fixing run is not part of the contract. A rerun is clean.
  let fixed = xsht(root, ["lint", "--only", "check.ignored-result", "--fix", "main.xsh", "helper.xsh"])?
  assert fixed.status.exited_with(0) or fixed.status.exited_with(2), fixed.stderr
  let rerun = xsht(root, ["lint", "--only", "check.ignored-result", "main.xsh", "helper.xsh"])?
  assert rerun.status.exited_with(0), rerun.stderr
  assert fp"{root}/main.xsh".read_text()? == "use helper\nproc data() [io] -> Int { print data; 1 }\nlet _ = data()\nif true { let _ = data() }\nmatch 1 {\n  1 => let _ = data(),\n  _ => {}\n}\nhelper.go()?\nprint done\n"
  assert fp"{root}/helper.xsh".read_text()? == "##! Helper module.\nproc data() [io] -> Int { print helper; 2 }\n## Run the helper.\nexport proc go() [io] {\n  let _ = data()\n  print go\n}\n"
  let ran = xsht(root, ["trace", "main.xsh"])?
  assert ran.status.exited_with(0), ran.stderr
  assert ran.stdout == "data\ndata\ndata\nhelper\ngo\ndone\n"

  let rejected = xsht(root, ["lint", "--only", "check.ignored-result", "--fix", "rejected.xsh"])?
  assert rejected.status.exited_with(2), rejected.stderr
  assert rejected.stderr.split("err[check.ignored-result]").len() == 3, rejected.stderr
  assert "`.push` returns a new list" in rejected.stderr, rejected.stderr
  # A copy update or a Result is never discarded mechanically.
  assert "help: discard" not in rejected.stderr, rejected.stderr
}

# A scoped fix rewrites only its diagnosed spans; unformatted code elsewhere
# keeps its exact bytes.
test test_lint_only_fix_leaves_bytes_outside_edited_spans { |ctx|
  let source = "let xs = [1,2,3]\nlet total: Int = xs.len()\nprint   f\"{total}\"  \nlet ys=[1, 2]\r\nprint f\"{ys.len()}\"\n"
  let root = test.temp_dir(ctx, name: "scoped-fix")?
  let script = fp"{root}/main.xsh"
  for case in [
    {
      rule: "lint.needless-annotation",
      expected: source.replace("let total: Int =", with: "let total ="),
    },
    {
      rule: "lint.prefer-const",
      expected: source.replace("let xs", with: "const xs").replace("let ys", with: "const ys"),
    },
  ] {
    script.write(source)
    let fixed = xsht(root, ["lint", "--only", case.rule, "--fix", "main.xsh"])?
    assert fixed.status.exited_with(0), f"{case.rule}: {fixed.stderr}"
    assert script.read_text()? == case.expected, case.rule
  }
}

test test_lint_only_rejects_unknown_codes { |ctx|
  # An empty directory: a selection that was wrongly accepted would lint it
  # and report nothing.
  let root = test.temp_dir(ctx, name: "unknown-codes")?
  for arguments in [["lint", "--only", "lint.prefer-const,lint.no-such-rule", "."], ["lint", "--only"]] {
    let rejected = xsht(root, arguments)?
    assert rejected.status.exited_with(2), rejected.stderr
    assert "--only" in rejected.stderr, rejected.stderr
  }

  let check_code = xsht(root, ["lint", "--only", "check.unresolved-name", "."])?
  assert "unknown lint rule 'check.unresolved-name'" in check_code.stderr, check_code.stderr
}

test test_lint_fix_applies_an_imported_module_edit_once { |ctx|
  let root = project(
    ctx,
    {
      "helper.xsh": "##! Helper module.\n## Exports a value.\nexport let value: Int = 1\n",
      "main.xsh": "use helper\nprint helper.value\n",
    },
  )?
  let fixed = xsht(root, ["lint", "--fix", "."])?
  assert fixed.status.exited_with(0), fixed.stderr
  assert fp"{root}/helper.xsh".read_text()? == "##! Helper module.\n## Exports a value.\nexport const value = 1\n"
}

test test_lint_entry_reachability_sees_imported_module_callables { |ctx|
  let root = project(
    ctx,
    {
      "helper.xsh": "##! Helper module.\n## Exports a value.\nexport let value = 1\n\npure unused() -> Int {\n  return 1\n}\n",
      "main.xsh": "use helper\nprint helper.value\n",
    },
  )?
  let linted = xsht(root, ["lint", "."])?
  assert linted.status.exited_with(1), linted.stderr
  assert linted.stderr.split("lint.unused-callable").len() == 2, linted.stderr
}

test test_lint_fix_converges_when_tail_edits_contain_named_argument_edits { |ctx|
  let fixture = p"tests/fixtures/syntax/valid/ergonomics-fix-convergence.xsh".read_text()?
  let root = project(ctx, {"fixture.xsh": fixture})?
  let before = xsht(root, ["trace", "fixture.xsh"])?
  assert before.status.exited_with(0), before.stderr
  let first = xsht(root, ["lint", "--fix", "fixture.xsh"])?
  assert first.status.exited_with(0), first.stderr
  let fixed = fp"{root}/fixture.xsh".read_text()?
  let second = xsht(root, ["lint", "--fix", "fixture.xsh"])?
  assert second.status.exited_with(0), second.stderr
  assert fp"{root}/fixture.xsh".read_text()? == fixed
  assert "words.join(separator:)" in fixed, fixed
  let after = xsht(root, ["trace", "fixture.xsh"])?
  assert after.status.exited_with(0), after.stderr
  assert before.stdout == after.stdout
}

# Arm bodies that differ only in spacing merge before and after `xsht fmt`:
# the linter reports the same arms of both matches whichever way the file is
# laid out.
test test_formatting_preserves_identical_match_arm_lints { |ctx|
  let root = project(
    ctx,
    {
      "arms.xsh": "const n = 2\nmatch n {\n  1 => print  \"small\"\n  2 => print \"small\"\n  else => print \"big\"\n}\nlet label = match n {\n  1 => [1,2]\n  2 => [1, 2]\n  else => []\n}\nprint f\"{label.len()}\"\n",
    },
  )?
  let merged = ["lint.identical-match-arms", "lint.identical-match-arms"]

  let written = xsht(root, ["lint"])?
  assert written.status.exited_with(1), written.stderr
  assert warning_codes(written.stderr) == merged, written.stderr
  assert "arms.xsh:3:3\n    1 => print  \"small\"\n" in written.stderr, written.stderr
  assert "arms.xsh:8:3\n    1 => [1,2]\n" in written.stderr, written.stderr

  let formatted = xsht(root, ["fmt"])?
  assert formatted.status.exited_with(0), formatted.stderr

  let relinted = xsht(root, ["lint"])?
  assert relinted.status.exited_with(1), relinted.stderr
  assert warning_codes(relinted.stderr) == merged, relinted.stderr
  assert "arms.xsh:4:3\n    1 => print \"small\"\n" in relinted.stderr, relinted.stderr
  assert "arms.xsh:10:3\n    1 => [1, 2],\n" in relinted.stderr, relinted.stderr
  assert "check.redundant-parens" not in relinted.stderr, relinted.stderr
}

# A lint gate is `xsht lint` over a tree being clean. A finding in a module
# that is only imported fails it, and the read-only command leaves every
# source as it was.
test test_lint_discovery_reports_imported_module_diagnostics_without_writing_sources { |ctx|
  let sources: Map[Str, Str] = {
    "xsht-config.ini": "module_path = .\n",
    "main.xsh": "use helper\nprint helper.value\n",
    "helper.xsh": "##! Helper module.\n## Exports a value.\nexport let value = 1\n\npure unused() -> Int {\n  return 1\n}\n",
  }
  let root = project(ctx, sources)?
  let linted = xsht(root, ["lint"])?
  assert linted.status.exited_with(1), linted.stderr
  assert "lint.unused-callable" in diagnostics(linted.stderr), linted.stderr
  for name in sources.keys() {
    assert fp"{root}/{name}".read_text()? == sources[name], f"read-only lint changed {name}"
  }
}

# Diagnostic fixtures are invalid on purpose. The config's `exclude` keeps
# them out of discovery, and without it the same tree fails on the fixture's
# checker error.
test test_lint_discovery_obeys_configured_fixture_exclusions { |ctx|
  let invalid_source = "let value: Int = \"wrong\"\n"
  let root = project(
    ctx,
    {
      "xsht-config.ini": "exclude = fixtures/**/*.xsh\n",
      "main.xsh": "print \"ready\"\n",
      "fixtures/invalid.xsh": invalid_source,
    },
  )?
  let clean = xsht(root, ["lint"])?
  assert clean.status.exited_with(0), clean.stderr
  assert clean.stdout == ""
  assert diagnostics(clean.stderr) == ""

  fp"{root}/xsht-config.ini".write("")
  let rejected = xsht(root, ["lint"])?
  assert rejected.status.exited_with(2), rejected.stderr
  assert "check.type-mismatch" in diagnostics(rejected.stderr), rejected.stderr
  assert fp"{root}/fixtures/invalid.xsh".read_text()? == invalid_source
}
