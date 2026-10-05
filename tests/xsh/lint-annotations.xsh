type Captured = {status: Status, stdout: Str, stderr: Str}

const rule = "lint.needless-annotation"

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# Writes `source` to `file`, applies the fixes of `only` alone, and returns
# the text the file then holds.
proc fixed_by(file: Path, only: Str, source: Str) [fs, process, env, error] -> Result[Str] {
  file.write(source)
  let fixed = lint(file, ["--fix", "--only", only])?
  assert fixed.status.exited_with(0) or fixed.status.exited_with(1) or fixed.status.exited_with(2), fixed.stderr
  file.read_text()
}

# Requires `file` to check without a diagnostic.
proc assert_checks(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

# Requires every annotation of the checked `source` to be reported with a fix
# that deletes it, giving `expected`, a checked and formatted program.
proc assert_annotations_removed(file: Path, source: Str, expected: Str, count: Int) [fs, process, env, error] {
  file.write(source)
  assert_checks(file)
  let reported = lint(file, ["--only", rule])?
  assert reported.status.exited_with(1), reported.stderr
  assert reported.stderr.split(f"warn[{rule}]").len() == count + 1, reported.stderr
  assert reported.stderr.split("help: remove needless annotation\n").len() == count + 1, reported.stderr

  assert fixed_by(file, rule, source)? == expected, source
  assert_checks(file)
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
  let again = lint(file, ["--only", rule])?
  assert again.status.exited_with(0), again.stderr
}

# Requires the checked `source` to keep its annotations: `only` is not
# reported and its fix leaves the text as written.
proc assert_retained(file: Path, only: Str, source: Str) [fs, process, env, error] {
  file.write(source)
  let reported = lint(file, ["--only", only])?
  assert "err[" not in reported.stderr, f"{source}{reported.stderr}"
  assert only not in reported.stderr, f"{source}{reported.stderr}"
  assert fixed_by(file, only, source)? == source, source
}

# Requires `source` to be rejected by the checker with `code`, and the
# annotation rule to stay silent and leave the text as written.
proc assert_unchecked_is_left(file: Path, source: Str, code: Str) [fs, process, env, error] {
  assert fixed_by(file, rule, source)? == source, source
  let reported = lint(file, ["--only", rule])?
  assert f"err[{code}]" in reported.stderr, reported.stderr
  assert rule not in reported.stderr, reported.stderr
}

test test_needless_str_annotation_fix_deletes_the_annotation { |ctx|
  let root = test.temp_dir(ctx, name: "needless-str")?
  assert_annotations_removed(fp"{root}/case.xsh", "let name: Str = \"pkg\"\n", "let name = \"pkg\"\n", 1)
}

test test_needless_scalar_annotations_each_have_a_fix { |ctx|
  let root = test.temp_dir(ctx, name: "needless-scalars")?
  assert_annotations_removed(
    fp"{root}/case.xsh",
    "let ok: Bool = true\nlet count: Int = 1\nlet ratio: Float = 3.14\nlet root: Path = p\"src\"\n",
    "let ok = true\nlet count = 1\nlet ratio = 3.14\nlet root = p\"src\"\n",
    4,
  )
}

test test_needless_list_annotations_are_reported { |ctx|
  let root = test.temp_dir(ctx, name: "needless-lists")?
  assert_annotations_removed(
    fp"{root}/case.xsh",
    "let deps: List[Str] = [\"musl\"]\nvar argv: List[Str] = [\"cc\", \"-O2\"]\nlet paths: List[Path] = [p\"src/main.c\", p\"lib/foo.c\"]\n",
    "let deps = [\"musl\"]\nvar argv = [\"cc\", \"-O2\"]\nlet paths = [p\"src/main.c\", p\"lib/foo.c\"]\n",
    3,
  )
}

test test_needless_export_str_annotation_has_a_fix { |ctx|
  let root = test.temp_dir(ctx, name: "needless-export")?
  assert_annotations_removed(fp"{root}/case.xsh", "export let rel: Str = \"1\"\n", "export let rel = \"1\"\n", 1)
}

# The annotation is what gives a dynamic method result its type, so it is not
# needless.
test test_needless_annotation_skips_a_dynamic_method_call_initializer { |ctx|
  let root = test.temp_dir(ctx, name: "needless-method")?
  let file = fp"{root}/case.xsh"
  assert_retained(
    file,
    rule,
    "let metadata: Map[Str, Any] = {\"name\": \"pkg\"}\nlet name: Str = metadata.get(\"name\").require()?\nprint \$name\n",
  )
  assert_unchecked_is_left(file, "let name: Str = metadata.get(\"name\")?\n", "check.unresolved-name")
}

test test_needless_annotation_skips_a_dynamic_module_call_initializer { |ctx|
  let root = test.temp_dir(ctx, name: "needless-module")?
  let file = fp"{root}/case.xsh"
  assert_retained(
    file,
    rule,
    "let rows: List[Record] = json.read(p\"index.json\")?.require()?\nprint \${rows.len()}\n",
  )
  assert_unchecked_is_left(file, "let rows: List[Record] = json.read(index)?\n", "check.unresolved-name")
}

test test_needless_annotation_skips_an_empty_list_initializer { |ctx|
  let root = test.temp_dir(ctx, name: "needless-empty-list")?
  assert_retained(fp"{root}/case.xsh", rule, "type Entry = {name: Str}\nvar entries: List[Entry] = []\n")
}

test test_needless_annotation_never_reports_proc_parameters { |ctx|
  let root = test.temp_dir(ctx, name: "needless-params")?
  assert_retained(fp"{root}/case.xsh", rule, "proc main(...argv: List[Str]) [error] {}\n")
}

test test_needless_var_annotation_has_a_fix { |ctx|
  let root = test.temp_dir(ctx, name: "needless-var")?
  assert_annotations_removed(fp"{root}/case.xsh", "var count: Int = 0\n", "var count = 0\n", 1)
}

# A propagated value whose field is then validated has no type until the
# annotation names one.
test test_needless_annotation_skips_a_dynamic_try_initializer { |ctx|
  let root = test.temp_dir(ctx, name: "needless-try")?
  let file = fp"{root}/case.xsh"
  assert_retained(
    file,
    rule,
    "type Named = {name: Any}\nlet raw: Any = {name: \"pkg\"}\nlet name: Str = raw.require(Named)?.name.require()?\nprint \$name\n",
  )
  assert_unchecked_is_left(file, "let name: Str = getenv(\"X\")?.display()\n", "check.unresolved-call")
}

test test_needless_annotation_fix_preserves_source { |ctx|
  let root = test.temp_dir(ctx, name: "needless-preserves")?
  assert_annotations_removed(
    fp"{root}/case.xsh",
    "let name: Str = \"pkg\"\nlet deps: List[Str] = [\"musl\", \"zlib\"]\nvar argv: List[Str] = [\"cc\", \"-O2\"]\nlet source_path: Path = p\"src/main.c\"\n",
    "let name = \"pkg\"\nlet deps = [\"musl\", \"zlib\"]\nvar argv = [\"cc\", \"-O2\"]\nlet source_path = p\"src/main.c\"\n",
    4,
  )
}

test test_needless_annotation_retains_contextual_collection_element_domains { |ctx|
  let root = test.temp_dir(ctx, name: "needless-domains")?
  for source in [
    "let values: List[Int?] = [null, 3]\nlet _ = values\n",
    "let expected: List[Int?] = [0, 2]\npure compare(actual: List[Int?]) { actual == expected }\n",
    "let expected: List[UInt] = [0, 2]\npure compare(actual: List[UInt]) { actual == expected }\n",
    "let expected: List[Int?] = [item for item in [0, 2]]\npure compare(actual: List[Int?]) { actual == expected }\n",
    "type Row = {value: Str?}\npure take(rows: List[Row]) {}\nlet rows: List[Row] = [{value: null}]\ntake(rows)\n",
  ] {
    assert_retained(fp"{root}/case.xsh", rule, source)
  }
}

test test_needless_annotation_retains_independent_inferred_require_boundary { |ctx|
  let root = test.temp_dir(ctx, name: "needless-require")?
  let file = fp"{root}/case.xsh"
  for source in [
    "let raw: Any = Ok(3)\nlet inner: Result[Int] = raw.require()?\nlet _ = inner\n",
    "let raw: Any = Ok(Ok(3))\nlet inner: Result[Result[Int]] = raw.require()?\nlet _ = inner\n",
    "let raw: Any = 3\nlet captured: Result[Int] = try { raw.require()? }\nlet _ = captured\n",
    "let raw: Any = 3\nlet value: Int = raw.require(Int)?\nlet _ = value\n",
  ] {
    assert_retained(file, rule, source)
    # Where the target can be inferred from the annotation, inferring it
    # leaves a program that still checks.
    let _ = fixed_by(file, "lint.inferred-require-target", source)?
    assert_checks(file)
  }
}

test test_needless_annotation_retains_empty_splice_element_anchor { |ctx|
  let root = test.temp_dir(ctx, name: "needless-splice")?
  for source in [
    "let empty: List[Str] = [@[], @[]]\npure consume(values: List[Str]) {}\nconsume(empty)\n",
    "let empty: List[Str] = [@[@[]], @[]]\npure consume(values: List[Str]) {}\nconsume(empty)\n",
  ] {
    assert_retained(fp"{root}/case.xsh", rule, source)
  }
}

test test_needless_annotation_retains_result_constructor_contract { |ctx|
  let root = test.temp_dir(ctx, name: "needless-result")?
  let file = fp"{root}/result-constructor.xsh"
  let declaration = "error Failure = Missing(message: Str)\n"
  for body in [
    "let failed: Result[Str, Failure] = Err(Failure.Missing(message: \"missing\"))\nlet recovered = failed ?? { |_error| \"fallback\" }\nlet _ = recovered\n",
    "let failed: Result[Bool, Failure] = Err(Failure.Missing(message: \"missing\"))\nlet recovered = failed ?? { |_error| false }\nlet _ = recovered\n",
    "let failed: Result[Unit, Failure] = Err(Failure.Missing(message: \"missing\"))\nlet _ = failed\n",
    "let success: Result[Str, Failure] = Ok(\"present\")\nlet _ = success\n",
  ] {
    for source in [declaration + body, declaration + "test result [error] { " + body + " }\n"] {
      assert_retained(file, rule, source)
      let all = lint(file, [])?
      assert rule not in all.stderr, f"{source}{all.stderr}"
    }
  }
}

test test_needless_annotation_retains_applied_nominal_element_anchor { |ctx|
  let root = test.temp_dir(ctx, name: "needless-nominal")?
  assert_retained(
    fp"{root}/case.xsh",
    rule,
    "type Row[T] = {value: T}\nproc gather() -> List[Row[Int]] { var rows: List[Row[Int]] = []; rows = rows.push(Row(value: 1)); rows }\n",
  )
}

test test_needless_annotation_retains_imported_nominal_element_anchor { |ctx|
  let root = test.temp_dir(ctx, name: "needless-imported")?
  fp"{root}/rows.xsh".write(
    "##! Row contracts.\n## A fixed integer row.\nexport type Row = {value: Int}\n## A row with a concrete value domain.\nexport type Box[T] = {value: T}\n",
  )
  let entry = fp"{root}/entry.xsh"
  for case in [{schema: "rows.Row", constructor: "rows.Row"}, {schema: "rows.Box[Int]", constructor: "rows.Box"}] {
    let source = "use rows\nproc gather() -> List[" + case.schema + "] { var values: List[" + case.schema + "] = []; values = values.push(" + case.constructor + "(value: 1)); values }\n"
    entry.write(source)
    assert_checks(entry)
    let all = lint(entry, [])?
    assert rule not in all.stderr, f"{source}{all.stderr}"
    assert entry.read_text()? == source
    assert_retained(entry, rule, source)
  }
}

# A signed literal is not proof of an unsigned value: the validation that
# rejects it at run time stays.
test test_redundant_require_retains_unsigned_validation { |ctx|
  let root = test.temp_dir(ctx, name: "require-unsigned")?
  let file = fp"{root}/unsigned.xsh"
  for source in [
    "let value = (-1).require(UInt)?\nlet _ = value\n",
    "let value = [1, -1].require(List[UInt])?\nlet _ = value\n",
  ] {
    assert_retained(file, "lint.redundant-require", source)
    assert_checks(file)
    let traced = run.capture --text "xsht" trace $file
    assert ! traced.status.exited_with(0), source
    assert "schema" in traced.stderr, traced.stderr
  }
}

test test_assertion_helper_fix_retains_contextual_collection_domains { |ctx|
  let root = test.temp_dir(ctx, name: "core-assert-domains")?
  let file = fp"{root}/case.xsh"
  for source in [
    "proc compare(actual: List[UInt]) { test.eq(actual, [3, 20])? }\n",
    "proc compare(actual: List[Int?]) { test.eq(actual, [3, 20])? }\n",
    "proc compare(actual: List[UInt]) { test.ne(right: [3, 20], left: actual)? }\n",
    "test unsigned [error] { let values: Map[UInt, Str] = {[3]: \"three\", [20]: \"twenty\"}; test.eq(values.keys(), [3, 20])? }\n",
  ] {
    file.write(source)
    assert_checks(file)
    let reported = lint(file, ["--only", "lint.core-assert"])?
    assert reported.stderr.split("warn[lint.core-assert]").len() == 2, f"{source}{reported.stderr}"
    # Whatever the fix rewrites, the operands keep the element domain the
    # helper's parameter gave them.
    let _ = fixed_by(file, "lint.core-assert", source)?
    assert_checks(file)
  }
}
