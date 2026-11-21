type Captured = {status: Status, stdout: Str, stderr: Str}

# The first line of each diagnostic in `stderr`, `SEVERITY[CODE]: MESSAGE`,
# in the order they were reported.
pure headers(stderr: Str) -> List[Str] {
  let found = collect {
    for line in stderr.lines() {
      yield line when rx"^(warn|err|note)\[[^\]]+\]: ".matches(line)
    }
  }

  found
}

# Runs `xsht lint` in `dir`, so the project there owns module resolution and
# test-root classification.
proc lint_dir(dir: Path, flags: List[Str]) [process, env, error] -> Result[Captured] {
  let linted = cd (dir) {
    run.capture --text "xsht" lint @flags .
  }?

  assert linted.status.exited_with(0) or linted.status.exited_with(1), linted.stderr
  Ok(linted)
}

test test_unused_export_follows_use_aliases_and_keeps_a_used_export { |ctx|
  let root = test.temp_dir(ctx, name: "unused-export-alias")?
  fp"{root}/helper.xsh".write(
    "##! Helper module.\n## Unreferenced.\nexport pure dead() -> Int {\n  return 1\n}\n\n## Referenced through an alias.\nexport pure used() -> Int {\n  return 2\n}\n",
  )
  fp"{root}/main.xsh".write("use helper as h\n\nprint h.used()\n")
  let reported = lint_dir(root, ["--only", "lint.unused-export"])?
  assert headers(reported.stderr) == ["warn[lint.unused-export]: unused export `dead`"], reported.stderr
  assert "helper.xsh:3:1\n" in reported.stderr, reported.stderr
}

test test_unused_export_separates_test_only_and_own_module_use { |ctx|
  let root = test.temp_dir(ctx, name: "unused-export-reduced")?
  fp"{root}/xsht-config.ini".write("module_path = .\ntest_roots = tests\n")
  fp"{root}/own.xsh".write(
    "##! Owning module.\n\n## An export only this module reads.\nexport pure own_only() -> Int {\n  return 1\n}\n\npure reads_own_only() -> Int {\n  return own_only()\n}\n",
  )
  fp"{root}/shared.xsh".write(
    "##! Shared values.\n\n## Only the test reads this.\nexport const test_only = 5\n\n## The entry reads this.\nexport const production_only = 6\n",
  )
  fp"{root}/tests".mkdir()
  fp"{root}/tests/use_shared.xsh".write(
    "use shared\n\ntest test_value {\n  assert shared.test_only == 5\n}\n",
  )
  fp"{root}/main.xsh".write("use shared\n\nprint \${shared.production_only}\n")
  let reported = lint_dir(root, ["--only", "lint.unused-export"])?
  assert reported.status.exited_with(0), reported.stderr
  let found = headers(reported.stderr)
  assert found.len() == 2, reported.stderr
  assert "note[lint.unused-export]: export `own_only` is used only inside its module" in found, reported.stderr
  assert "note[lint.unused-export]: export `test_only` is used only by tests" in found, reported.stderr
}

test test_unused_export_keeps_same_named_exports_in_two_modules_distinct { |ctx|
  let root = test.temp_dir(ctx, name: "unused-export-same-name")?
  fp"{root}/first".mkdir()
  fp"{root}/second".mkdir()
  fp"{root}/first/types.xsh".write(
    "##! First types.\n\n## The used target.\nexport type Thing = {name: Str}\n",
  )
  fp"{root}/second/types.xsh".write(
    "##! Second types.\n\n## The unreferenced target.\nexport type Thing = {name: Str}\n",
  )
  fp"{root}/main.xsh".write(
    "use first.types as a\n\nlet value: a.Thing = {name: \"x\"}\nprint \${value.name}\n",
  )
  let reported = lint_dir(root, ["--only", "lint.unused-export"])?
  assert headers(reported.stderr) == ["warn[lint.unused-export]: unused export `Thing`"], reported.stderr
  assert "second/types.xsh:4:1\n" in reported.stderr, reported.stderr
  assert "first/types.xsh" not in reported.stderr, reported.stderr
}

test test_unused_export_follows_a_reexport_chain { |ctx|
  let root = test.temp_dir(ctx, name: "unused-export-reexport")?
  fp"{root}/base.xsh".write(
    "##! Base limits.\n\n## The relayed limit.\nexport const base_limit = 8\n",
  )
  fp"{root}/relay.xsh".write(
    "##! Relayed limits.\nuse base\n\n## The relayed limit.\nexport const relayed: Int = base.base_limit\n",
  )
  fp"{root}/main.xsh".write("use relay\n\nprint \${relay.relayed}\n")
  let reported = lint_dir(root, ["--only", "lint.unused-export"])?
  assert reported.status.exited_with(0), reported.stderr
  assert headers(reported.stderr) == [], reported.stderr
}

test test_unused_export_exempts_configured_public_api_modules { |ctx|
  let root = test.temp_dir(ctx, name: "unused-export-public-api")?
  fp"{root}/xsht-config.ini".write(
    "module_path = .\n\n[dead-code]\npublic-api = api/**/*.xsh\n",
  )
  fp"{root}/api".mkdir()
  fp"{root}/api/library.xsh".write(
    "##! Published library.\n\n## A promised value.\nexport const dead_api_value = 1\n\n## A promised helper.\nexport pure dead_api_helper() -> Int {\n  return 2\n}\n",
  )
  fp"{root}/app.xsh".write(
    "##! Unpublished application.\n\n## An unreferenced export.\nexport pure dead_app() -> Int {\n  return 3\n}\n",
  )
  let reported = lint_dir(root, ["--only", "lint.unused-export"])?
  assert headers(reported.stderr) == ["warn[lint.unused-export]: unused export `dead_app`"], reported.stderr
  assert "api/library.xsh" not in reported.stderr, reported.stderr
}

# The fixture copies the shapes a repository-wide audit found in a real
# project: eighteen exports no module references and two unread private
# constants, beside used exports, aliased imports, same-named exports in two
# modules, a re-export chain, a test-only export, an own-module-only export,
# and a configured public API.
test test_unused_export_reproduces_project_shapes {
  let fixture = p"tests/fixtures/lint/unused-export/laputa"
  let reported = lint_dir(fixture, ["--only", "lint.unused-export"])?
  let unused = collect {
    for header in headers(reported.stderr) {
      yield header when header.starts_with("warn[lint.unused-export]: unused export ")
    }
  }
  assert unused.len() == 18, reported.stderr
  for name in [
    "with_remote_name",
    "UnusedShape",
    "UnusedReceipt",
    "UnusedPlan",
    "parse_size_bytes",
    "REMOTE_REPLY_LIMIT",
    "store_key",
    "store_digest",
    "STORE_FORMAT",
    "StoreEntry",
    "StoreReceipt",
    "publish_store",
    "plan_action_is_build",
    "plan_action_label",
    "PLAN_FORMAT",
    "PlanStep",
    "PlanReceipt",
    "Target",
  ] {
    assert f"warn[lint.unused-export]: unused export `{name}`" in unused, reported.stderr
  }
  # A referenced export, a module reached through an alias, a type used in a
  # signature, an enum used through a variant, and a re-exported constant stay
  # live.
  for name in [
    "`load`",
    "`format_size`",
    "`REMOTE_RETRY_LIMIT`",
    "`relayed_limit`",
    "`shared_limit`",
    "`Catalog`",
    "`Kind`",
  ] {
    assert f"unused export {name}" not in reported.stderr, reported.stderr
  }
  # Only the system `Target` is unreferenced; the package `Target` the entry
  # annotates is not.
  assert reported.stderr.split("unused export `Target`").len() == 2, reported.stderr
  # The published interface is exempt, and the reduced-use categories are
  # reported apart from the unreferenced exports.
  assert "dead_api_value" not in reported.stderr, reported.stderr
  assert "dead_api_helper" not in reported.stderr, reported.stderr
  assert "note[lint.unused-export]: export `REMOTE_TEST_ONLY_LIMIT` is used only by tests" in reported.stderr, reported.stderr
  assert "note[lint.unused-export]: export `own_only` is used only inside its module" in reported.stderr, reported.stderr

  let constants = lint_dir(fixture, ["--only", "lint.unused-const"])?
  let unread = headers(constants.stderr)
  assert unread.len() == 2, constants.stderr
  assert "warn[lint.unused-const]: unused constant `case_dir`" in unread, constants.stderr
  assert "warn[lint.unused-const]: unused constant `nul`" in unread, constants.stderr
}
