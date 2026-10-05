type Captured = {status: Status, stdout: Str, stderr: Str}

const legacy_choice_module = "##! Choices.\n## A nominal choice.\nexport type Choice = Selected(Int) | Empty # retained café\n"

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
    run.capture --text "xsht" @arguments ?
  }
}

# Runs `xsht` with `arguments` in `root` and requires it to succeed.
proc xsht_ok(root: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  let output = xsht(root, arguments)?
  assert output.status.exited_with(0), output.stderr
  Ok(output)
}

test test_mixed_enum_and_record_require_migration_rechecks_import_graph_and_converges_in_stages { |ctx|
  let root = project(
    ctx,
    {
      "choice.xsh": legacy_choice_module,
      "entry.xsh": "use choice as c\n## A name.\nexport type Name = {name: Str}\nlet _ = record.require({name: \"café\"}, {name: \"Str\"})? # retained receiver\nlet choice: c.Choice = c.Selected(7)\nprint \"café\"\nmatch choice { c.Selected(number) => print \$number; c.Empty => print \"empty\" }\n",
    },
  )?
  let entry = fp"{root}/entry.xsh"
  let choices = fp"{root}/choice.xsh"
  let before = xsht(root, ["check", "entry.xsh"])?
  assert ! before.status.exited_with(0), before.stderr
  let _ = xsht_ok(root, ["lint", "--fix", "entry.xsh"])?
  let fixed_entry = entry.read_text()?
  let fixed_module = choices.read_text()?
  assert "{name: \"café\"}.require(Name)? # retained receiver" in fixed_entry, fixed_entry
  assert "export enum Choice {" in fixed_module, fixed_module
  assert "# retained café" in fixed_module, fixed_module
  let _ = xsht_ok(root, ["check", "entry.xsh"])?
  let executed = xsht_ok(root, ["trace", "entry.xsh"])?
  assert executed.stdout == "café\n7\n"

  # Exact syntax/API repair makes ordinary lints available on the next pass;
  # they can then remove identity schema validation and normalize layout.
  let _ = xsht_ok(root, ["lint", "--fix", "entry.xsh"])?
  let canonical_entry = entry.read_text()?
  let canonical_module = choices.read_text()?
  assert "# retained receiver" in canonical_entry, canonical_entry
  assert "# retained café" in canonical_module, canonical_module
  let after = xsht_ok(root, ["trace", "entry.xsh"])?
  assert executed.stdout == after.stdout
  let _ = xsht_ok(root, ["lint", "--fix", "entry.xsh"])?
  assert entry.read_text()? == canonical_entry
  assert choices.read_text()? == canonical_module
}

test test_mixed_enum_and_record_require_migration_refuses_unproved_identity { |ctx|
  let module_source = "##! Choices.\n## A nominal choice.\nexport type Choice = Selected(Int) | Empty\n"
  let entry_source = "use choice as c\ntype Name = {name: Str}\nlet _ = record.require({name: 7}, {name: \"Str\"})?\nlet choice: c.Choice = c.Selected(7)\n"
  let root = project(ctx, {"choice.xsh": module_source, "entry.xsh": entry_source})?
  let fixed = xsht(root, ["lint", "--fix", "entry.xsh"])?
  assert ! fixed.status.exited_with(0), fixed.stderr
  assert "lint.removed-record-require" in fixed.stdout + fixed.stderr, fixed.stdout + fixed.stderr
  assert fp"{root}/entry.xsh".read_text()? == entry_source
  assert fp"{root}/choice.xsh".read_text()? == module_source
}

test test_mixed_enum_and_record_require_migration_refuses_unrelated_import_graph_errors { |ctx|
  let entry_source = "use choice as c\ntype Name = {name: Str}\nlet _ = record.require({name: \"café\"}, {name: \"Str\"})?\nlet choice: c.Choice = c.Selected(7)\nprint \"café\"\n"
  let broken = "let broken: Int = \"wrong\"\n"
  for case in [
    {module: legacy_choice_module, entry: entry_source + broken, code: "check.type-mismatch"},
    {module: legacy_choice_module + broken, entry: entry_source, code: "check.type-mismatch"},
    # A removed API has no executable result type. Its consumer errors
    # remain checker failures, even when the call has an identity fix.
    {
      module: legacy_choice_module,
      entry: entry_source.replace("let _ =", "let value =") + "print \$value.name\n",
      code: "check.field-access",
    },
  ] {
    let root = project(ctx, {"choice.xsh": case.module, "entry.xsh": case.entry})?
    let fixed = xsht(root, ["lint", "--fix", "entry.xsh"])?
    assert ! fixed.status.exited_with(0), fixed.stderr
    assert case.code in fixed.stderr, fixed.stderr
    assert fp"{root}/entry.xsh".read_text()? == case.entry
    assert fp"{root}/choice.xsh".read_text()? == case.module
  }
}

test test_removed_record_require_fix_rechecks_and_converges_in_stages { |ctx|
  let root = project(
    ctx,
    {
      "entry.xsh": "export type Name = {name: Str}\nconst required = {name: \"Str\"}\nlet value = record.require({name: \"café\", extra: 7}, required)?\nprint \$value.name\n",
    },
  )?
  let entry = fp"{root}/entry.xsh"
  let before = xsht(root, ["check", "entry.xsh"])?
  assert before.status.exited_with(2), before.stderr
  assert "check.removed-record-require" in before.stderr, before.stderr
  let _ = xsht_ok(root, ["lint", "--fix", "entry.xsh"])?
  let fixed = entry.read_text()?
  assert "record.require" not in fixed, fixed
  assert "café" in fixed or "caf\\u{e9}" in fixed, fixed
  let _ = xsht_ok(root, ["check", "entry.xsh"])?
  let before_ordinary_fixes = xsht_ok(root, ["trace", "entry.xsh"])?
  let _ = xsht_ok(root, ["lint", "--fix", "entry.xsh"])?
  let canonical = entry.read_text()?
  let _ = xsht_ok(root, ["lint", "--fix", "entry.xsh"])?
  assert entry.read_text()? == canonical
  let executed = xsht_ok(root, ["trace", "entry.xsh"])?
  assert before_ordinary_fixes.stdout == executed.stdout
  assert "café" in executed.stdout, executed.stdout
}

test test_removed_record_require_fix_preserves_unrelated_errors_and_comments { |ctx|
  for source in [
    "type Name = {name: Str}\nlet value = record.require({name: \"demo\"}, {name: \"Str\"})?\nlet broken: Int = \"wrong\"\n",
    "type Name = {name: Str}\nlet value = record.require({name: \"demo\"}, # retained café\n {name: \"Str\"})?\n",
  ] {
    let root = project(ctx, {"entry.xsh": source})?
    let fixed = xsht(root, ["lint", "--fix", "entry.xsh"])?
    assert ! fixed.status.exited_with(0), fixed.stderr
    assert fp"{root}/entry.xsh".read_text()? == source
  }
}

test test_membership_migration_after_removal_fixes_shared_import_once_and_is_idempotent { |ctx|
  let entry = "use helper\nassert helper.present(\"needle\")\n"
  let root = project(
    ctx,
    {
      "helper.xsh": "##! Membership fixture.\n## Tests membership.\nexport pure present(text: Str) -> Bool { return text.contains(\"needle\") }\n",
      "first.xsh": entry,
      "second.xsh": entry,
    },
  )?
  let helper = fp"{root}/helper.xsh"
  let removed = xsht(root, ["check", "first.xsh"])?
  assert removed.status.exited_with(2), removed.stderr
  assert "check.removed-membership" in removed.stderr, removed.stderr
  let _ = xsht_ok(root, ["lint", "--fix", "first.xsh", "second.xsh"])?
  let text = helper.read_text()?
  assert "\"needle\" in text" in text, text
  let _ = xsht_ok(root, ["lint", "--fix", "first.xsh", "second.xsh"])?
  assert helper.read_text()? == text
}

test test_membership_migration_does_not_suppress_unrelated_checker_failure { |ctx|
  let root = project(
    ctx,
    {"main.xsh": "let result: Int = \"abc\".contains(\"a\")\nlet count: Int = \"invalid\"\n"},
  )?
  let fixed = xsht(root, ["lint", "--fix", "main.xsh"])?
  assert fixed.status.exited_with(2), fixed.stderr
  assert "check.type-mismatch" in fixed.stderr, fixed.stderr
}

test test_regex_literal_lint_fixture_preserves_execution_and_is_idempotent { |ctx|
  let source = r"""let assignment = regex.compile("^([A-Z]+)=([0-9]+)$")? # keep
print ${assignment.matches("COUNT=42")} ${assignment.captures("COUNT=42")[2]} ${assignment.replace("COUNT=42", "$2")}
"""
  let root = project(ctx, {"regex.xsh": source})?
  let fixture = fp"{root}/regex.xsh"
  let before = xsht_ok(root, ["trace", "regex.xsh"])?
  let _ = xsht_ok(root, ["lint", "--fix", "regex.xsh"])?
  let fixed = fixture.read_text()?
  assert r"""rx"^([A-Z]+)=([0-9]+)$" # keep""" in fixed, fixed
  let _ = xsht_ok(root, ["lint", "--fix", "regex.xsh"])?
  assert fixture.read_text()? == fixed
  let after = xsht_ok(root, ["trace", "regex.xsh"])?
  assert before.stdout == after.stdout
}

test test_private_pure_return_annotation_mode_and_lint_policy_preserve_each_other { |ctx|
  let root = project(
    ctx,
    {
      "main.xsh": "pure label(name: Str) { name.trim() }\nprint label(\"ready\")\n",
      "xsht-config.ini": "[check]\nannotate = returns\n[lint]\nprefer-inferred-pure-returns = true\n",
    },
  )?
  let script = fp"{root}/main.xsh"
  let _ = xsht_ok(root, ["check", "--annotate", "main.xsh"])?
  let annotated = script.read_text()?
  assert "-> Str" in annotated, annotated
  let _ = xsht_ok(root, ["lint", "--fix", "main.xsh"])?
  assert script.read_text()? == annotated
}

test test_private_pure_return_annotation_removal_is_opt_in_and_idempotent { |ctx|
  let source = "pure label(name: Str) -> Str { name.trim() }\nprint label(\"ready\")\n"
  let root = project(ctx, {"main.xsh": source})?
  let script = fp"{root}/main.xsh"
  let _ = xsht_ok(root, ["lint", "--fix", "main.xsh"])?
  assert script.read_text()? == source
  fp"{root}/xsht-config.ini".write("[lint]\nprefer-inferred-pure-returns = true\n")
  let _ = xsht_ok(root, ["lint", "--fix", "main.xsh"])?
  let fixed = script.read_text()?
  assert "-> Str" not in fixed, fixed
  let _ = xsht_ok(root, ["lint", "--fix", "main.xsh"])?
  assert script.read_text()? == fixed
}

test test_value_pipeline_hole_lint_converges_without_changing_execution { |ctx|
  let root = project(
    ctx,
    {
      "main.xsh": "pure first(value: Int) -> Int { value + 1 }\npure second(prefix: Int, value: Int) -> Int { prefix + value }\npure third(value: Int) -> Int { value * 2 }\nlet initial = first(2)\nlet next = second(10, value: initial)\nlet final_value = third(next)\nprint \$final_value\n",
    },
  )?
  let script = fp"{root}/main.xsh"
  let before = xsht_ok(root, ["trace", "main.xsh"])?
  let _ = xsht_ok(root, ["lint", "--fix", "main.xsh"])?
  let fixed = script.read_text()?
  assert "first(2) |> second(10, value: _) |> third(_)" in fixed, fixed
  let _ = xsht_ok(root, ["lint", "--fix", "main.xsh"])?
  assert script.read_text()? == fixed
  let after = xsht_ok(root, ["trace", "main.xsh"])?
  assert before.stdout == after.stdout
}

test test_enum_migration_fix_preserves_comments_aliases_and_imports { |ctx|
  let root = project(
    ctx,
    {
      "choice.xsh": "##! Nominal choices.\n## A choice.\nexport type Choice =\n  Selected(Int) # selected café\n  | Empty # absent\n## Same nominal identity.\nexport type Alias = Choice\n",
      "entry.xsh": "use choice as c\nlet value: c.Alias = c.Selected(7)\nmatch value { c.Selected(number) => print \$number; c.Empty => print \"empty\" }\n",
    },
  )?
  let choices = fp"{root}/choice.xsh"
  let rejected = xsht(root, ["check", "entry.xsh"])?
  assert ! rejected.status.exited_with(0), rejected.stderr
  assert "parse.enum-migration" in rejected.stderr, rejected.stderr
  let _ = xsht_ok(root, ["lint", "--fix", "entry.xsh"])?
  let fixed = choices.read_text()?
  for fragment in ["export enum Choice {", "# selected café", "# absent", "export type Alias = Choice"] {
    assert fragment in fixed, fixed
  }

  let _ = xsht_ok(root, ["lint", "--fix", "entry.xsh"])?
  assert choices.read_text()? == fixed
  let _ = xsht_ok(root, ["check", "entry.xsh"])?
}

test test_enum_migration_fix_retains_unrelated_checker_errors { |ctx|
  let source = "type Choice = Selected(Int) | Empty\nlet value = Selected(\"wrong\")\n"
  let root = project(ctx, {"entry.xsh": source})?
  let fixed = xsht(root, ["lint", "--fix", "entry.xsh"])?
  assert ! fixed.status.exited_with(0), fixed.stderr
  assert "check.type-mismatch" in fixed.stderr, fixed.stderr
  assert fp"{root}/entry.xsh".read_text()? == source
}

test test_signature_cli_safe_fix_preserves_process_results_and_is_idempotent { |ctx|
  let source = r"""type Options = {jobs: Int, verbose: Bool}
proc main(...argv: List[Str]) [error] {
  let {jobs, verbose}: Options = cli.parse(argv, {jobs: {kind: "Int", default: 4, help: "Int, default: 4"}, verbose: {kind: "Bool", default: false, help: "Bool, default: false"}})?
  let shown = Options(jobs:, verbose:)
  print $shown.jobs $shown.verbose
}
"""
  let root = project(ctx, {"entry.xsh": source})?
  let fixture = fp"{root}/entry.xsh"
  let cases: List[List[Str]] = [[], ["--jobs=8", "--verbose"], ["--help"], ["--jobs=invalid"], ["--jobs=2", "--jobs=3"]]
  var before = []
  for arguments in cases {
    let output = xsht(root, ["trace", "entry.xsh", "--", @arguments])?
    before += [{status: output.status.exit_code()?, stdout: output.stdout}]
  }

  let _ = xsht_ok(root, ["lint", "--fix", "entry.xsh"])?
  let fixed = fixture.read_text()?
  assert "cli main(" in fixed, fixed
  let _ = xsht_ok(root, ["lint", "--fix", "entry.xsh"])?
  assert fixture.read_text()? == fixed
  for index, arguments in cases {
    let output = xsht(root, ["trace", "entry.xsh", "--", @arguments])?
    assert output.status.exit_code()? == before[index].status, arguments.join(" ")
    assert output.stdout == before[index].stdout, arguments.join(" ")
  }
}
