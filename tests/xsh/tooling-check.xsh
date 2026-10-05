const with_error_handler = r"""proc main(...argv: List[Str]) [error] -> Result[Unit] {
  with value = fallible() {
    print ${value}
  } else { |err|
    return Err(err)
  }
  return Ok()
}

proc fallible() [error] -> Result[Str] {
  return "ok"
}
"""

const unchecked_boundary = r"""
type Row = {name: Str}
let row: Row = json.decode("{\"name\":\"demo\"}")?
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

# What `xsht check` or `xsht lint` printed on standard error before its
# closing timing line.
pure diagnostics(stderr: Str) -> Str {
  let found = rx"(?s)^(.*)xsht [a-z]+: [0-9]+ files? in [^\n]* \(thread time by stage: [^\n]*\n$".captures(stderr)
  if found.len() == 2 { found[1] } else { f"no timing line: {stderr}" }
}

# Requires `xsht check TARGET`, run in a project of `files`, to succeed with
# nothing on standard output and no diagnostics.
proc expect_clean_check(ctx: TestContext, files: Map[Str, Str], target: Str) [fs, process, env, error] {
  let root = project(ctx, files)?
  cd $root {
    let checked = run.capture --text "xsht" check $target
    assert checked.status.exited_with(0), checked.stderr
    assert checked.stdout == ""
    assert diagnostics(checked.stderr) == "", checked.stderr
  }
}

# Requires `xsht check TARGET`, run in a project of `files`, to succeed.
proc expect_check_passes(ctx: TestContext, files: Map[Str, Str], target: Str) [fs, process, env, error] {
  let root = project(ctx, files)?
  cd $root {
    let checked = run.capture --text "xsht" check $target
    assert checked.status.exited_with(0), checked.stderr
  }
}

test test_check_uses_shared_pipeline {
  let checked = run.capture --text "xsht" check tests/fixtures/runtime/cli-simple.xsh
  assert checked.status.exited_with(0), checked.stderr
  assert checked.stdout == ""
  assert diagnostics(checked.stderr) == "", checked.stderr
}

test test_check_defaults_to_current_directory_and_respects_excludes { |ctx|
  let root = project(
    ctx,
    {"ok.xsh": "let value = 1\n", "ignored/bad.xsh": "let =\n", "xsht-config.ini": "exclude = ignored/**\n"},
  )?
  cd $root {
    let checked = run.capture --text "xsht" check
    assert checked.status.exited_with(0), checked.stderr
    assert checked.stdout == ""
    assert diagnostics(checked.stderr) == "", checked.stderr
  }
}

test test_check_accepts_directories_and_reports_failures { |ctx|
  let root = project(ctx, {"scripts/ok.xsh": "let value = 1\n", "scripts/bad.xsh": "let =\n"})?
  cd $root {
    let checked = run.capture --text "xsht" check scripts
    assert checked.status.exited_with(2), checked.stderr
    assert checked.stdout == ""
    assert "parse.expected-ident" in checked.stderr, checked.stderr
  }
}

test test_check_dynamic_boundaries_are_default { |ctx|
  let file = test.temp_file(ctx, name: "boundary.xsh", contents: bytes.from_text(unchecked_boundary))?
  let strict = run.capture --text "xsht" check --strict $file
  assert strict.status.exited_with(2), strict.stderr
  assert "`xsht check --strict` was removed" in strict.stderr, strict.stderr

  let normal = run.capture --text "xsht" check $file
  assert normal.status.exited_with(2), normal.stderr
  assert "err[check.dynamic-boundary]" in normal.stderr, normal.stderr
}

test test_check_strict_option_reports_default_dynamic_policy_before_loading { |ctx|
  let root = test.temp_dir(ctx, name: "removed-option")?
  cd $root {
    let strict = run.capture --text "xsht" check --strict missing.xsh
    assert strict.status.exited_with(2), strict.stderr
    assert strict.stdout == ""
    assert "`xsht check --strict` was removed" in strict.stderr, strict.stderr
    assert "dynamic boundaries are checked by default" in strict.stderr, strict.stderr
    assert "failed to read" not in strict.stderr, strict.stderr
  }

  let help = run.capture --text "xsht" check --help
  assert help.status.exited_with(0), help.stderr
  assert "--strict" not in help.stdout, help.stdout
}

test test_check_dynamic_boundary_rejects_disk_fixture_without_annotation_writes { |ctx|
  let source = p"tests/fixtures/sema/invalid/unchecked-json-boundary.xsh".read_text()?
  let root = project(ctx, {"boundary.xsh": source})?
  cd $root {
    let checked = run.capture --text "xsht" check --annotate boundary.xsh
    assert checked.status.exited_with(2), checked.stderr
    assert checked.stdout == ""
    assert "err[check.dynamic-boundary]" in checked.stderr, checked.stderr
  }

  assert fp"{root}/boundary.xsh".read_text()? == source
}

test test_check_annotate_rewrites_safe_annotations { |ctx|
  let file = test.temp_file(
    ctx,
    name: "annotate.xsh",
    contents: bytes.from_text(r"""
let count=1
var names=["a", "b"]
let _ = process.command_argv("echo", ["ok"])
let data: Any = json.decode("{}")?
let row = {name: "demo"}
export let label="demo"
proc local(input = Path(".")) {}
export proc entry(flag = true) {}
"""),
  )?
  let annotated = run.capture --text "xsht" check --annotate $file
  assert annotated.status.exited_with(0), annotated.stderr
  assert annotated.stdout == ""
  assert diagnostics(annotated.stderr) == "", annotated.stderr
  assert file.read_text()? == r"""let count = 1
var names = ["a", "b"]
let _ = process.command_argv("echo", ["ok"])
let data: Any = json.decode("{}")?
let row = {name: "demo"}

export let label: Str = "demo"

proc local(input: Path = Path(".")) {}

export proc entry(flag: Bool = true) -> Result[Unit] {}
"""
}

test test_check_annotate_locals_rewrites_local_shapes { |ctx|
  let file = test.temp_file(
    ctx,
    name: "annotate-locals.xsh",
    contents: bytes.from_text("""
let count = 1
let names = ["a", "b"]
let command = process.command_argv("echo", names)
"""),
  )?
  let annotated = run.capture --text "xsht" check --annotate=locals $file
  assert annotated.status.exited_with(0), annotated.stderr
  assert annotated.stdout == ""
  assert diagnostics(annotated.stderr) == "", annotated.stderr
  assert file.read_text()? == """let count = 1
let names: List[Str] = ["a", "b"]
let command: Command = process.command_argv("echo", names)
"""
}

test test_check_annotate_uses_exact_configured_classes { |ctx|
  let source = """
let names = ["a", "b"]
export let label = "demo"
proc local(input = Path(".")) {}
"""
  let root = project(ctx, {"xsht-config.ini": "[check]\nannotate = locals\n  exports\n", "main.xsh": source})?
  cd $root {
    let annotated = run.capture --text "xsht" check --annotate main.xsh
    assert annotated.status.exited_with(0), annotated.stderr
    assert annotated.stdout == ""
    assert diagnostics(annotated.stderr) == "", annotated.stderr
  }

  assert fp"{root}/main.xsh".read_text()? == """let names: List[Str] = ["a", "b"]

export let label: Str = "demo"

proc local(input = Path(".")) {}
"""
}

test test_check_annotate_skips_unsafe_or_unhelpful_types { |ctx|
  let source = """let {name} = {name: "demo"}
let data = json.decode("{}")?
let row = {name: "demo"}
"""
  let file = test.temp_file(ctx, name: "annotate-skips.xsh", contents: bytes.from_text(source))?
  let annotated = run.capture --text "xsht" check --annotate=locals $file
  assert annotated.status.exited_with(0), annotated.stderr
  assert annotated.stdout == ""
  assert diagnostics(annotated.stderr) == "", annotated.stderr
  assert file.read_text()? == source
}

test test_check_annotate_rewrites_only_requested_script { |ctx|
  let helper = "##! Annotate helper module.\n## Exposes the imported value.\nexport let value = 1\n"
  let root = project(
    ctx,
    {"helper.xsh": helper, "main.xsh": "use helper\nproc local(input = Path(\".\")) {}\nlet names = [\"a\"]\n"},
  )?
  let annotated = run.capture --text "xsht" check --annotate fp"{root}/main.xsh"
  assert annotated.status.exited_with(0), annotated.stderr
  assert annotated.stdout == ""
  assert diagnostics(annotated.stderr) == "", annotated.stderr
  assert fp"{root}/helper.xsh".read_text()? == helper
  assert fp"{root}/main.xsh".read_text()? == "use helper\n\nproc local(input: Path = Path(\".\")) {}\n\nlet names = [\"a\"]\n"
}

test test_check_annotate_does_not_write_on_dynamic_boundary_errors { |ctx|
  let file = test.temp_file(ctx, name: "annotate-boundary.xsh", contents: bytes.from_text(unchecked_boundary))?
  let annotated = run.capture --text "xsht" check --annotate $file
  assert annotated.status.exited_with(2), annotated.stderr
  assert "err[check.dynamic-boundary]" in annotated.stderr, annotated.stderr
  assert file.read_text()? == unchecked_boundary
}

test test_check_annotate_uses_xsht_config_line_width { |ctx|
  let root = project(
    ctx,
    {
      "xsht-config.ini": "[format]\nline-width = 60\n",
      "main.xsh": "proc local(input = Path(\".\"), source = Path(\".\"), destination = Path(\".\")) {}\n",
    },
  )?
  cd $root {
    let annotated = run.capture --text "xsht" check --annotate main.xsh
    assert annotated.status.exited_with(0), annotated.stderr
  }

  assert fp"{root}/main.xsh".read_text()? == """proc local(
  input: Path = Path("."),
  source: Path = Path("."),
  destination: Path = Path("."),
) {}
"""
}

test test_check_reveals_type_without_failing { |ctx|
  let file = test.temp_file(
    ctx,
    name: "reveal.xsh",
    contents: bytes.from_text("""
let names = ["a", "b"]
reveal_type(names)
"""),
  )?
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  assert checked.stdout == ""
  assert "note[check.reveal-type]: revealed type: List[Str]" in checked.stderr, checked.stderr
}

test test_xsh_rejects_reveal_type { |ctx|
  test.expect(ctx, "reveal_type(1)\n", status: 2, stderr: ["err[check.reveal-type]"])?
}

test test_check_rejects_undefined_utility_commands { |ctx|
  let file = test.temp_file(ctx, name: "utility.xsh", contents: bytes.from_text("echo hi\n"))?
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(2), checked.stderr
  assert checked.stdout == ""
  assert "err[check.unresolved-proc-command]" in checked.stderr, checked.stderr
  assert "unresolved proc command" in checked.stderr, checked.stderr
}

test test_check_ignores_xshi_config_aliases { |ctx|
  let config = """{
  aliases: [
    {name: "echo", source: "print"},
  ],
}
"""
  let home = project(ctx, {".config/xshi/config.xsh": config})?
  let file = test.temp_file(ctx, name: "alias.xsh", contents: bytes.from_text("echo hi\n"))?
  env HOME=$home {
    let checked = run.capture --text "xsht" check $file
    assert checked.status.exited_with(2), checked.stderr
    assert checked.stdout == ""
    assert "err[check.unresolved-proc-command]" in checked.stderr, checked.stderr
  }

  assert ! fp"{home}/.local/share/xshi/history".exists()?
}

test test_check_accepts_indexed_with_error_handlers { |ctx|
  expect_clean_check(ctx, {"main.xsh": with_error_handler}, "main.xsh")
}

# These assert rendered source locations across files.
test test_check_attributes_imported_parse_error_to_its_source { |ctx|
  let root = project(
    ctx,
    {"main.xsh": "use helper as h\nprint tui.red()\n", "helper.xsh": "##! Helper.\nexport let value =\n"},
  )?
  cd $root {
    let checked = run.capture --text "xsht" check main.xsh
    assert checked.status.exited_with(2), checked.stderr
    assert "main.xsh:1:1" in checked.stderr, checked.stderr
    assert "helper.xsh:2:19" in checked.stderr, checked.stderr
    assert "parse.expected-expression" in checked.stderr, checked.stderr
    assert "<xsh-stdlib:" not in checked.stderr, checked.stderr
  }
}

test test_check_accepts_imported_lazy_default_with_embedded_module_loaded { |ctx|
  expect_clean_check(
    ctx,
    {
      "main.xsh": "use helper as h\nprint tui.red()\nlet value = h.scan()\n",
      "helper.xsh": "##! Helper.\n## Returns a value.\nexport pure scan(x: Int = 1 / 0) -> Int {\n  return x\n}\n",
    },
    "main.xsh",
  )
}

test test_check_reports_public_standard_call_name_at_user_source { |ctx|
  let root = project(ctx, {"main.xsh": "print tui.red(1)\n"})?
  cd $root {
    let checked = run.capture --text "xsht" check main.xsh
    assert checked.status.exited_with(2), checked.stderr
    assert "check.arity" in checked.stderr, checked.stderr
    assert "main.xsh:1:15" in checked.stderr, checked.stderr
    assert "tui.red(1)" in checked.stderr, checked.stderr
    assert "<xsh-stdlib:" not in checked.stderr, checked.stderr
  }
}

test test_check_explicit_directory_uses_directory_config { |ctx|
  let root = project(
    ctx,
    {
      "xsht-config.ini": "exclude = project/bad.xsh\n",
      "project/xsht-config.ini": "",
      "project/bad.xsh": "let value =\n",
    },
  )?
  cd $root {
    let checked = run.capture --text "xsht" check project
    assert checked.status.exited_with(2), checked.stderr
    assert "parse.expected-expression" in checked.stderr, checked.stderr
  }
}

# Explicit directory selection is a CLI discovery boundary: configured extra
# roots apply to the default scan, not to a directory the caller named.
test test_check_explicit_directory_does_not_expand_parent_config_includes { |ctx|
  let root = project(
    ctx,
    {
      "xsht-config.ini": "include = extra\n",
      "project/main.xsh": "let value = 1\n",
      "extra/bad.xsh": "let value =\n",
    },
  )?
  cd $root {
    let checked = run.capture --text "xsht" check project
    assert checked.status.exited_with(0), checked.stderr
  }
}

test test_check_summary_groups_directory_failures_by_code { |ctx|
  let root = project(ctx, {"project/parse.xsh": "let value =\n", "project/lower.xsh": with_error_handler})?
  cd $root {
    let checked = run.capture --text "xsht" check --summary project
    assert checked.status.exited_with(2), checked.stderr
    assert "parse.expected-expression" in checked.stderr, checked.stderr
    assert "compact.indexed-build" not in checked.stderr, checked.stderr
    assert "xsht check summary:" in checked.stderr, checked.stderr
    assert "parse.expected-expression: 1" in checked.stderr, checked.stderr
  }
}

test test_check_directory_accepts_indexed_with_error_handlers { |ctx|
  expect_clean_check(ctx, {"project/ok.xsh": "let value = 1\n", "project/bad.xsh": with_error_handler}, "project")
}

test test_check_top_level_user_imports_are_skippable_for_lowerability { |ctx|
  let root = project(
    ctx,
    {
      "project/helper.xsh": "##! Helper module.\n## Exposes a test value.\nexport let value = 1\n",
      "project/pm/make.xsh": "##! Make helper module.\n## Exposes configured jobs.\nexport let jobs = 1\n",
      "project/PKGBUILD-shared.xsh": "##! Shared package module.\n## Exposes the package name.\nexport let pkgname = \"demo\"\n",
      "project/main.xsh": "use helper as h\nuse pm.make as make\nuse PKGBUILD-shared as PKGBUILD_shared\n",
    },
  )?
  cd $root {
    let checked = run.capture --text "xsht" check project/main.xsh
    assert checked.status.exited_with(0), checked.stderr
  }
}

test test_check_accepts_lazy_default_in_main_dependency { |ctx|
  let source = """pure helper(x: Int = 1 / 0) -> Int {
  return x
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let _ = helper()
  return Ok()
}
"""
  expect_clean_check(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_accepts_lazy_default_in_nested_top_level_call { |ctx|
  let source = """pure scan_corpus(x: Int = 1 / 0) -> Int {
  return x
}

let report = {corpus: scan_corpus()}
"""
  expect_clean_check(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_main_dependency_lowers_result_context_chain { |ctx|
  let source = """error AppletError = Usage(message: Str)

pure common_int(raw: Str) -> Result[Int] {
  match raw {
    "1" => 1
    _ => raw.parse_int().context("usage", "bad int")?
  }
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let _ = common_int("2")?
  return Ok()
}
"""
  expect_clean_check(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_for_line_item_allows_str_methods { |ctx|
  let source = r"""proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let text = "a=b\nc=d"
  for line in text.lines() {
    let parts = line.split("=")
    print ${parts.len()}
  }
  return Ok()
}
"""
  expect_check_passes(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_local_method_chain_through_if_binding { |ctx|
  let source = r"""pure lookup(body: Str, name: Str) -> Str {
  for raw in body.lines() {
    let stripped = raw.trim()
    let line = if stripped.starts_with("export ") { (stripped.split("export ").get(1) ?? "").trim() } else { stripped }
    if line.starts_with(f"{name}=") {
      return (line.split("=").get(1) ?? "").trim().replace("\"", with: "").replace("'", with: "")
    }
  }
  return ""
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let _ = lookup("export A=1", "A")
  return Ok()
}
"""
  expect_check_passes(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_match_ok_binding_allows_str_methods { |ctx|
  let source = r"""proc read_summary(candidate: Path) [fs, error] -> Result[Str] {
  match candidate.read_text() {
    Ok(text_value) => {
      let lines = text_value.lines().collect()
      let summary = lines[1].trim()
      return summary
    }
    Err(_) => {}
  }
  ""
}

proc main(...argv: List[Str]) [fs, error] -> Result[Unit] {
  let _ = read_summary(path.absolute("x")?)?
  return Ok()
}
"""
  expect_check_passes(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_run_text_binding_allows_str_methods { |ctx|
  let source = r"""proc main(...argv: List[Str]) [process, error] -> Result[Unit] {
  let out = run.text printf hello ?
  print ${out.trim()}
  return Ok()
}
"""
  expect_check_passes(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_explicit_list_annotation_validates_any_result_binding { |ctx|
  let source = r"""proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let stored: Record = {deps: []}
  let deps: List[Str] = stored.get("deps")?.require(List[Str])?
  print "deps" deps.len() deps.join(" ")
  return Ok()
}
"""
  expect_check_passes(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_par_map_result_item_flows_to_for_loop { |ctx|
  let source = r"""type BuiltPackage = {metadata_sha256: Str}
type Package = {name: Str}

proc build_world_package(pkg: Package) [error] -> Result[List[BuiltPackage]] {
  return Ok([{metadata_sha256: pkg.name}])
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let pending: List[Package] = [{name: "demo"}]
  let built_batches = pending |> par-map(jobs: 1) { |pkg| build_world_package(pkg) }
  for built in built_batches {
    let outcome: Result[List[BuiltPackage]] = built
    let packages = outcome?
    print ${packages.len()}
    print ${packages[0].metadata_sha256}
  }
  return Ok()
}
"""
  expect_check_passes(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_where_pipeline_preserves_item_type_for_loop { |ctx|
  let source = r"""proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let words = "a b".split(" ") |> where .trim() != ""
  for word in words {
    print ${word.trim()}
  }
  return Ok()
}
"""
  expect_check_passes(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_nested_par_map_enumerate_preserves_line_type { |ctx|
  let source = r"""type Finding = {line: Int, text: Str}

proc read_source(src: Str) [error] -> Result[Str] {
  return Ok(src)
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let sources = [" alpha\n beta "]
  let findings: List[Finding] = sources
    |> par-map { |source|
      var hits: List[Finding] = []

      match read_source(source) {
        Ok(src) => {
          for item in src.lines() |> enumerate() {
            let line_num = item.index + 1
            let line = item.value
            hits = hits.push({line: line_num, text: line.trim()})
          }
        }
        Err(_) => {}
      }

      hits
    }
    |> flat-map { |hits|
      hits
    }

  print ${findings.len()}
  return Ok()
}
"""
  expect_check_passes(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_path_property_field_flows_to_method_call { |ctx|
  let source = r"""proc main(...argv: List[Str]) [fs, error] -> Result[Unit] {
  let dir = fs.cwd()?
  let parent = dir.parent
  print ${parent.display()}
  return Ok()
}
"""
  expect_check_passes(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_dynamic_record_get_requires_explicit_validation { |ctx|
  let source = r"""proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let exports: Any = {sources: ["a", "b"]}
  let checked = exports.require(Record)?
  if "sources" in checked {
    let sources: List[Str] = checked.get("sources")?.require(List[Str])?
    print ${sources.len()}
  }
  return Ok()
}
"""
  expect_check_passes(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_fs_walk_map_path_result_flows_to_for_loop { |ctx|
  let source = r"""proc main(...argv: List[Str]) [fs, error] -> Result[Unit] {
  let dest = p"."
  let manifest = fs.walk(dest)
    |> where .kind == "file" or .kind == "symlink"
    |> map { |entry|
      entry.path.strip_prefix(dest)?
    }
    |> sort-by .display()
  for rel_path in manifest {
    let key = rel_path.display()
    print ${key}
  }
  return Ok()
}
"""
  expect_check_passes(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_compact_lowerability_accepts_lowered_record_methods { |ctx|
  let source = r"""proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let exports: Record = {sources: {name: "demo"}}
  let sources = exports.get("sources")?.require(Record)?
  if sources.keys().len() != 0 {
    return Ok()
  }
  return Ok()
}
"""
  expect_check_passes(ctx, {"main.xsh": source}, "main.xsh")
}

test test_check_local_binding_can_shadow_import_capture_for_lowerability { |ctx|
  let source = r"""use remote

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let remote = "local"
  print ${remote.trim()}
  return Ok()
}
"""
  let root = project(
    ctx,
    {"remote.xsh": "##! Remote module.\n## Exposes the imported value.\nexport let value = 1\n", "main.xsh": source},
  )?
  cd $root {
    let checked = run.capture --text "xsht" check main.xsh
    assert checked.status.exited_with(0), checked.stderr
  }
}

test test_check_rejects_main_without_spread_parameter_but_accepts_spread { |ctx|
  let root = project(
    ctx,
    {
      "nonspread.xsh": "proc main(argv: List[Str]) [env, error] {\n  print \"hello\"\n}\n",
      "spread.xsh": "proc main(...argv: List[Str]) [fs, env, error] {\n  print \"hello\"\n}\n",
    },
  )?
  cd $root {
    let nonspread = run.capture --text "xsht" check nonspread.xsh
    assert nonspread.status.exited_with(2), nonspread.stderr
    assert "compact.main-missing-spread" in nonspread.stderr, nonspread.stderr
    assert "spread form `(...argv: List[Str])`" in nonspread.stderr, nonspread.stderr

    let spread = run.capture --text "xsht" check spread.xsh
    assert spread.status.exited_with(0), spread.stderr
  }
}

test test_check_rejects_unreachable_invalid_regex_literals_without_execution { |ctx|
  let root = project(ctx, {"invalid.xsh": "print \"must not execute\"\npure unused() -> Regex { rx\"(\" }\n"})?
  cd $root {
    let checked = run.capture --text "xsht" check invalid.xsh
    assert ! checked.status.exited_with(0), checked.stderr
    assert "must not execute" not in checked.stdout, checked.stdout
    assert "check.regex-literal" in checked.stderr, checked.stderr
    assert "invalid.xsh:2:" in checked.stderr, checked.stderr
  }
}

test test_check_validates_regex_literals_in_unused_imported_functions { |ctx|
  let root = project(
    ctx,
    {
      "main.xsh": "use broken\nprint \"must not execute\"\n",
      "broken.xsh": "pure never_called() -> Regex { rx\"[\" }\n",
    },
  )?
  cd $root {
    let checked = run.capture --text "xsht" check main.xsh
    assert ! checked.status.exited_with(0), checked.stderr
    assert "check.regex-literal" in checked.stderr, checked.stderr
    assert "broken.xsh:1:" in checked.stderr, checked.stderr
  }
}

test test_check_imported_error_annotation_retains_constructor_identity { |ctx|
  let root = project(
    ctx,
    {
      "helper.xsh": "##! Error identity fixture.\n## An exported failure family.\nexport error HelperError = Failed(detail: Str) : Temporary\n",
      "main.xsh": "use helper\nlet failure: helper.HelperError = helper.HelperError.Failed(detail: \"failed\")\n",
    },
  )?
  cd $root {
    let checked = run.capture --text "xsht" check main.xsh
    assert checked.status.exited_with(0), checked.stderr
  }
}

test test_check_rejects_duplicate_test_names_and_callable_collisions { |ctx|
  let root = test.temp_dir(ctx, name: "collision")?
  let source = fp"{root}/collision.xsh"
  for text in [
    "test same {}\ntest same {}",
    "pure same() -> Int { 1 }\ntest same {}",
    "let same = 1\ntest same {}",
    "test same {}\nlet {same} = {same: 1}",
    "use env as same\ntest same {}",
  ] {
    source.write(text)
    let checked = run.capture --text "xsht" check $source
    assert checked.status.exited_with(2), f"{text}: {checked.stderr}"
    assert "check.duplicate-name" in checked.stderr, f"{text}: {checked.stderr}"
  }

  source.write("use helper\ntest entry {}\n")
  for text in ["test same {}\ntest same {}", "test same {}\npure same() -> Int { 1 }", "let same = 1\ntest same {}"] {
    fp"{root}/helper.xsh".write(text)
    let checked = run.capture --text "xsht" check $source
    assert checked.status.exited_with(2), f"{text}: {checked.stderr}"
    assert "check.duplicate-name" in checked.stderr, f"{text}: {checked.stderr}"
  }
}

# The check and lint workers must hold a deeply nested schema constructor on
# their default stacks, so the tools run here with no `RUST_MIN_STACK` to
# enlarge them.
test test_check_and_lint_nested_schema_constructors_without_stack_environment { |ctx|
  var source = "type Leaf = {value: Int}\n"
  var previous = "Leaf"
  var constructor = "Leaf(value: 1)"
  for depth in range(40) {
    let name = f"Layer{depth}"
    source = source + "type " + name + " = {child: " + previous + "}\n"
    constructor = name + "(child: " + constructor + ")"
    previous = name
  }

  source = source + "pure build() -> " + previous + " { " + constructor + " }\nlet value = build()\nlet _ = value\n"
  let root = project(ctx, {"nested.xsh": source})?
  cd $root {
    let checked = run.capture --text env -u RUST_MIN_STACK -u XSH_MODULE_PATH xsht check nested.xsh
    assert checked.status.exited_with(0), checked.stderr
    assert "stack overflow" not in checked.stderr, checked.stderr
    assert p"nested.xsh".read_text()? == source

    let linted = run.capture --text env -u RUST_MIN_STACK -u XSH_MODULE_PATH xsht lint nested.xsh
    assert linted.status.exited_with(0) or linted.status.exited_with(1), linted.stderr
    assert "stack overflow" not in linted.stderr, linted.stderr
    assert p"nested.xsh".read_text()? == source
  }
}

# A script that is not UTF-8 is a diagnostic naming the problem, not a crash
# in the loader.
test test_check_reports_invalid_utf8_source_as_a_diagnostic { |ctx|
  let script = test.temp_file(ctx, name: "invalid-utf8.xsh", contents: b"let \xff")?
  let checked = run.capture --text "xsht" check $script
  assert checked.status.exited_with(2), checked.stderr
  assert "source.invalid-utf8" in checked.stderr, checked.stderr
}
