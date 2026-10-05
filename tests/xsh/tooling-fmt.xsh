const long_list = "let values = [\"alpha\", \"beta\", \"gamma\", \"delta\", \"epsilon\", \"zeta\"]\n"

# A module whose first doc comment is attached to no export, which draws
# `check.orphan-doc-comment` once however many scripts import it.
const orphan_doc_helper = "##! Helper module.\n## This comment is not attached to an export.\nlet value = 1\n\n## Exports a value.\nexport let exported: Int = value\n"

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

test test_fmt_check_accepts_stable_examples {
  let checked = run.capture --text "xsht" fmt --check tests/fixtures/runtime/cli-simple.xsh \
    tests/fixtures/runtime/cli-args.xsh
  assert checked.status.exited_with(0), checked.stderr
  assert checked.stdout == ""
  assert checked.stderr == ""
}

test test_fmt_writes_canonical_source { |ctx|
  let file = test.temp_file(
    ctx,
    name: "writes.xsh",
    contents: bytes.from_text("proc main(args:List[Str])->Result[Unit]{return Ok()}\n"),
  )?
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  assert formatted.stdout == ""
  assert formatted.stderr == ""
  # A short body written on one line stays on one line.
  assert file.read_text()? == "proc main(args: List[Str]) -> Result[Unit] { return Ok() }\n"
}

test test_fmt_check_reports_unformatted_files { |ctx|
  let file = test.temp_file(ctx, name: "unformatted.xsh", contents: bytes.from_text("let x=1\n"))?
  let checked = run.capture --text "xsht" fmt --check $file
  assert checked.status.exited_with(1), checked.stderr
  assert "needs formatting" in checked.stdout, checked.stdout
  assert checked.stderr == ""
}

test test_fmt_check_reports_discovered_files_in_stable_order { |ctx|
  let root = project(ctx, {"b.xsh": "let b=1\n", "a.xsh": "let a=1\n"})?
  cd $root {
    let checked = run.capture --text "xsht" fmt --check
    assert checked.status.exited_with(1), checked.stderr
    let around_first = checked.stdout.split("a.xsh: needs formatting")
    assert around_first.len() == 2, checked.stdout
    assert "b.xsh: needs formatting" in around_first[1], checked.stdout
    assert checked.stderr == ""
  }
}

test test_fmt_uses_nearest_xsht_config_line_width { |ctx|
  let root = project(
    ctx,
    {
      "xsht-config.ini": "[format]\nline-width = 120\n",
      "narrow/xsht-config.ini": "[format]\nline-width = 60\n",
      "narrow/main.xsh": long_list,
    },
  )?
  let script = fp"{root}/narrow/main.xsh"
  cd $root {
    let formatted = run.capture --text "xsht" fmt $script
    assert formatted.status.exited_with(0), formatted.stderr
  }

  assert script.read_text()? == """let values = [
  "alpha",
  "beta",
  "gamma",
  "delta",
  "epsilon",
  "zeta",
]
"""
}

test test_fmt_discovery_skips_format_excludes_but_formats_named_files { |ctx|
  let unformatted = "let  value = 1\n"
  let root = project(
    ctx,
    {
      "xsht-config.ini": "[format]\nexclude = snippets/**\n",
      "snippets/one.xsh": unformatted,
      "main.xsh": unformatted,
    },
  )?
  cd $root {
    let discovered = run.capture --text "xsht" fmt --check
    assert discovered.status.exited_with(1), discovered.stdout
    assert "main.xsh: needs formatting" in discovered.stdout, discovered.stdout
    assert "one.xsh" not in discovered.stdout, discovered.stdout

    let named = run.capture --text "xsht" fmt --check snippets/one.xsh
    assert named.status.exited_with(1), named.stdout
    assert "one.xsh: needs formatting" in named.stdout, named.stdout
  }
}

test test_fmt_explicit_directory_formats_xsh_files { |ctx|
  let root = project(
    ctx,
    {"project/main.xsh": "let values=[1,2,3]\n", "project/nested/helper.xsh": "let values=[4,5,6]\n"},
  )?
  cd $root {
    let formatted = run.capture --text "xsht" fmt fp"{root}/project"
    assert formatted.status.exited_with(0), formatted.stderr
  }

  assert fp"{root}/project/main.xsh".read_text()? == "let values = [1, 2, 3]\n"
  assert fp"{root}/project/nested/helper.xsh".read_text()? == "let values = [4, 5, 6]\n"
}

test test_fmt_checks_imported_modules { |ctx|
  let root = project(
    ctx,
    {
      "helper.xsh": "##! Invalid helper module.\n## Deliberately returns the wrong type.\nexport pure bad() -> Int {\n  return \"not an int\"\n}\n",
      "main.xsh": "use helper\nprint helper.bad()\n",
    },
  )?
  cd $root {
    let formatted = run.capture --text "xsht" fmt main.xsh
    assert formatted.status.exited_with(2), formatted.stderr
    assert "helper.xsh" in formatted.stderr, formatted.stderr
    assert "check.type-mismatch" in formatted.stderr, formatted.stderr
  }
}

test test_fmt_deduplicates_diagnostics_from_imported_modules { |ctx|
  let entry = "use helper\nprint helper.exported\n"
  let root = project(ctx, {"helper.xsh": orphan_doc_helper, "first.xsh": entry, "second.xsh": entry})?
  cd $root {
    let formatted = run.capture --text "xsht" fmt .
    assert formatted.status.exited_with(2), formatted.stderr
    # The imported module's diagnostic is rendered once.
    assert formatted.stderr.split("check.orphan-doc-comment").len() == 2, formatted.stderr
  }
}

test test_fmt_reports_invalid_xsht_config_line_width { |ctx|
  let root = project(ctx, {"xsht-config.ini": "[format]\nline-width = nope\n", "main.xsh": "let value = 1\n"})?
  cd $root {
    let checked = run.capture --text "xsht" fmt --check main.xsh
    assert checked.status.exited_with(2), checked.stderr
    assert "format.line-width" in checked.stderr, checked.stderr
  }
}

test test_fmt_ignores_legacy_config_ini { |ctx|
  let root = project(ctx, {"config.ini": "[format]\nline-width = 60\n", "main.xsh": long_list})?
  cd $root {
    let checked = run.capture --text "xsht" fmt --check main.xsh
    assert checked.status.exited_with(0), checked.stderr
  }
}
