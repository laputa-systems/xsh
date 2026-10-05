type Plugin = module {
  export let name: Str
  export optional let description: Str
  export proc execute(root: Path) [fs, error] -> Result[Unit, Error]
}

test test_module_load { |ctx|
  let root = test.temp_dir(ctx, name: "module")?
  let plugin_path = fp"{root}/plugin.xsh"

  fs.write(
    plugin_path,
    """
##! Test plugin module contract.

## Exposes the plugin name.
export let name: Str = "demo"
## Exposes the optional plugin description.
export let description: Str = "loaded module"

## Writes the plugin name into the requested root.
export proc execute(root: Path) [fs, error] -> Result[Unit] {
  fs.write(fp"{root}/out.txt", name)?
}
""",
  )?

  let plugin = module.load(plugin_path)?.require(Plugin)?
  assert plugin.name == "demo"
  assert "description" in plugin.keys()
  assert "missing" not in plugin.keys()
  assert "name" in plugin.keys()
  assert plugin.keys().len() == 3
  plugin.execute(root)?
  assert fp"{root}/out.txt".read_text()? == "demo"
}

type Builder = module {
  export pure label(value: Str) -> Str
  export proc build(root: Path) [fs, error] -> Result[Path, Error]
}

# A callable export is called the same way on a contract-checked module
# whether the module was bound to a name first or reached through `?.`.
test test_module_export_call_chains_after_require { |ctx|
  let root = test.temp_dir(ctx, name: "chained-export-call")?
  let builder_path = fp"{root}/builder.xsh"
  builder_path.write("""##! Test builder module.

## Labels a value.
export pure label(value: Str) -> Str {
  f"built-{value}"
}

## Writes the build marker and returns its path.
export proc build(root: Path) [fs, error] -> Result[Path] {
  let marker = fp"{root}/marker.txt"
  marker.write(label("marker"))?
  marker
}
""")?

  let bound = module.load(builder_path)?.require(Builder)?
  let bound_marker = bound.build(root)?
  let chained_marker = module.load(builder_path)?.require(Builder)?.build(root)?
  assert chained_marker == bound_marker
  assert chained_marker.read_text()? == "built-marker"
  assert module.load(builder_path)?.require(Builder)?.label("x") == bound.label("x")
}

type MissingOnly = module {
  export let name: Str
  export optional let description: Str
  export let version: Int
  export pure render(value: Str) -> Str
}

type MismatchedOnly = module {
  export let name: Int
  export pure label(value: Str, suffix: Str) -> Str
  export proc build(root: Str) [fs, error] -> Result[Path, Error]
}

type EffectsAndReturn = module {
  export proc build(root: Path) [fs, process, error] -> Result[Path, Error]
  export pure label(value: Str) -> Int
}

type WrongKinds = module {
  export proc label(value: Str) -> Str
  export let build: Str
  export pure name() -> Str
}

type MissingAndMismatched = module {
  export let name: Int
  export let version: Int
}

proc contract_fixture(ctx: TestContext) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "contract-report")?
  let fixture = fp"{root}/fixture.xsh"
  fixture.write("""##! Contract report fixture.

## The fixture name.
export let name: Str = "fixture"

## Labels a value.
export pure label(value: Str) -> Str {
  f"label-{value}"
}

## Returns the root it was given.
export proc build(root: Path) [fs, error] -> Result[Path] {
  root.mkdir()?
  root
}
""")?
  fixture
}

# A failed contract check names every violation in one error, with the
# expected and found signatures, and its facets say which categories occurred.
test test_module_contract_failure_names_missing_exports { |ctx|
  let fixture = contract_fixture(ctx)?
  match module.load(fixture)?.require(MissingOnly) {
    Ok(_) => test.fail("a module without required exports satisfied the contract")?
    Err(error) => {
      assert error is MissingExport
      assert ! (error is MismatchedExport)
      assert "missing export `version`: expected `export let version: Int`" in error.message, error.message
      assert "missing export `render`: expected `export pure render(value: Str) -> Str`" in error.message, error.message
      assert "description" not in error.message, error.message
      assert "`name`" not in error.message, error.message
      assert "expected Module, found Module" not in error.message, error.message
    }
  }
}

test test_module_contract_failure_names_mismatched_exports { |ctx|
  let fixture = contract_fixture(ctx)?
  match module.load(fixture)?.require(MismatchedOnly) {
    Ok(_) => test.fail("a module with other signatures satisfied the contract")?
    Err(error) => {
      assert error is MismatchedExport
      assert ! (error is MissingExport)
      assert "mismatched export `name`: expected `export let name: Int`, found `export let name: Str` (the value type differs)" in error.message, error.message
      assert "mismatched export `label`: expected `export pure label(value: Str, suffix: Str) -> Str`, found `export pure label(value: Str) -> Str` (the contract declares 2 parameters, the export takes 1 parameter)" in error.message, error.message
      assert "mismatched export `build`: expected `export proc build(root: Str) [fs, error] -> Result[Path, Error]`, found `export proc build(root: Path) [fs, error] -> Result[Path, Error]` (parameter 1 has type Path, the contract declares Str)" in error.message, error.message
    }
  }

  match module.load(fixture)?.require(EffectsAndReturn) {
    Ok(_) => test.fail("a module with other effects satisfied the contract")?
    Err(signature) => {
      assert signature is MismatchedExport
      assert "(the effects are [fs, error], the contract declares [fs, process, error])" in signature.message, signature.message
      assert "(the return type is Str, the contract declares Int)" in signature.message, signature.message
    }
  }

  match module.load(fixture)?.require(WrongKinds) {
    Ok(_) => test.fail("a module with other export kinds satisfied the contract")?
    Err(kinds) => {
      assert kinds is MismatchedExport
      assert "(the contract declares a proc, the module exports a pure function)" in kinds.message, kinds.message
      assert "(the contract declares a value, the module exports a callable)" in kinds.message, kinds.message
      assert "(the contract declares a callable, the module exports a value)" in kinds.message, kinds.message
    }
  }
}

test test_module_contract_failure_reports_both_categories { |ctx|
  let fixture = contract_fixture(ctx)?
  match module.load(fixture)?.require(MissingAndMismatched) {
    Ok(_) => test.fail("an unsatisfied contract succeeded")?
    Err(error) => {
      assert error is MissingExport
      assert error is MismatchedExport
      assert "mismatched export `name`" in error.message, error.message
      assert "missing export `version`" in error.message, error.message
      test.error_kind(error, "schema")?
    }
  }
}

type ExactFixture = exact module {
  export let name: Str
  export optional let description: Str
  export pure label(value: Str) -> Str
  export proc build(root: Path) [fs, error] -> Result[Path, Error]
}

type ExactTooSmall = exact module {
  export let name: Str
  export optional let description: Str
}

type ExactAllCategories = exact module {
  export let name: Int
  export let version: Int
}

# An exact contract is the module's whole surface: the same module passes when
# every export is listed (an optional one may be absent) and fails, naming each
# extra export, when the contract lists fewer.
test test_exact_module_contract_rejects_unexpected_exports { |ctx|
  let fixture = contract_fixture(ctx)?
  let exact = module.load(fixture)?.require(ExactFixture)?
  assert exact.label("x") == "label-x"

  match module.load(fixture)?.require(ExactTooSmall) {
    Ok(_) => test.fail("a module with unlisted exports satisfied an exact contract")?
    Err(error) => {
      assert error is UnexpectedExport
      assert ! (error is MissingExport)
      assert ! (error is MismatchedExport)
      assert "unexpected export `build`: `export proc build(root: Path) [fs, error] -> Result[Path, Error]` is not in the exact contract" in error.message, error.message
      assert "unexpected export `label`: `export pure label(value: Str) -> Str` is not in the exact contract" in error.message, error.message
      assert "`name`" not in error.message, error.message
      assert "description" not in error.message, error.message
      test.error_kind(error, "schema")?
    }
  }

  match module.load(fixture)?.require(ExactAllCategories) {
    Ok(_) => test.fail("an unsatisfied exact contract succeeded")?
    Err(error) => {
      assert error is MissingExport
      assert error is MismatchedExport
      assert error is UnexpectedExport
      assert "mismatched export `name`" in error.message, error.message
      assert "missing export `version`" in error.message, error.message
      assert "unexpected export `build`" in error.message, error.message
      assert "unexpected export `label`" in error.message, error.message
    }
  }

  # The open form of the same small contract still allows the extras.
  let _ = module.load(fixture)?.require(OpenNameOnly)?
}

type OpenNameOnly = module {
  export let name: Str
}

# A statically imported module satisfies an exact contract only when it
# exports nothing else, and a value typed by an open contract never does.
test test_exact_module_contract_is_checked_statically { |ctx|
  let root = test.temp_dir(ctx, name: "exact-static")?
  fp"{root}/service.xsh".write("""##! Service fixture.

## The service name.
export let name: Str = "cache"

## An export the small contract does not list.
export pure extra() -> Int {
  1
}
""")?
  let env_root = {XSH_MODULE_PATH: root.display()}

  let accepted = test.run_script(
    ctx,
    r"""type Service = exact module {
  export let name: Str
  export pure extra() -> Int
  export optional let description: Str
}

use service

proc main() [io] {
  let checked: Service = service
  print ${checked.name} ${checked.extra()}
}
""",
    [],
    env_root,
  )?
  assert accepted.success, accepted.stderr
  assert accepted.stdout == "cache 1\n", accepted.stdout

  let rejected = test.run_script(
    ctx,
    """type Service = exact module {
  export let name: Str
}

use service

let checked: Service = service
""",
    [],
    env_root,
  )?
  assert rejected.status == 2, rejected.stderr
  assert "check.type-mismatch" in rejected.stderr, rejected.stderr
  assert "unexpected export `extra`: the exact contract does not list it" in rejected.stderr, rejected.stderr

  let widened = test.run_script(
    ctx,
    """type Open = module {
  export let name: Str
}

type Service = exact module {
  export let name: Str
}

use service

let open: Open = service
let checked: Service = open
""",
    [],
    env_root,
  )?
  assert widened.status == 2, widened.stderr
  assert "check.type-mismatch" in widened.stderr, widened.stderr
  assert "an exact contract needs `.require(Contract)`" in widened.stderr, widened.stderr

  # Outside a contract position `exact` stays an ordinary name.
  let ordinary = test.run_script(
    ctx,
    r"""let exact = 3
type Count = Int
let count: Count = exact
print $count
""",
  )?
  assert ordinary.success, ordinary.stderr
  assert ordinary.stdout == "3\n", ordinary.stdout
}

type VersionedModule = module {
  export let version: Int
}

# Loads share prepared modules across evaluators in one process; a rewritten
# module must not reuse the earlier preparation. Each outer module's export
# harvest runs in a fresh child evaluator that loads the versioned file.
test test_module_load_reprepares_a_rewritten_module_in_another_evaluator { |ctx|
  let root = test.temp_dir(ctx, name: "rewritten-module")?
  let versioned = fp"{root}/versioned.xsh"
  let module_text = """##! Versioned module.
## Exposes the version.
export let version = """
  let outer_text = f"""##! Reloads the versioned module.
type VersionedModule = module {{
  export let version: Int
}}

## Exposes the reloaded version.
export let version = module.load(fp"{versioned}")?.require(VersionedModule)?.version
"""
  let first = fp"{root}/first.xsh"
  let second = fp"{root}/second.xsh"
  first.write(outer_text)?
  second.write(outer_text)?

  versioned.write(
    module_text + """1
""",
  )?
  assert module.load(first)?.require(VersionedModule)?.version == 1
  versioned.write(
    module_text + """2
""",
  )?
  assert module.load(second)?.require(VersionedModule)?.version == 2
}

# Loading a module again in the same evaluator sees its current file: an
# unchanged module keeps its exports, and a rewritten one is reloaded.
test test_module_load_reloads_a_changed_module_in_the_same_evaluator { |ctx|
  let root = test.temp_dir(ctx, name: "reloaded-module")?
  let versioned = fp"{root}/versioned.xsh"
  let module_text = """##! Versioned module.
## Exposes the version.
export let version = """
  versioned.write(module_text + "1\n")?
  assert module.load(versioned)?.require(VersionedModule)?.version == 1
  assert module.load(versioned)?.require(VersionedModule)?.version == 1
  versioned.write(module_text + "2\n")?
  assert module.load(versioned)?.require(VersionedModule)?.version == 2
}

test test_module_load_exports_private_fields_and_contract_errors { |ctx|
  let root = test.temp_dir(ctx, name: "dynamic-module-contract")?
  fp"{root}/helper.xsh".write(r"""
##! Dynamic helper module.
## Exposes the helper name.
export let helper_name = "demo"

## Renders a helper label.
export pure helper_label(value: Str) -> Str {
  return f"helper:{value}"
}
""")?
  let package = fp"{root}/package.xsh"
  package.write(r"""
##! Dynamic package module.
use helper

let prefix = helper.helper_name

pure label_private(value: Str) -> Str {
  return f"{prefix}-{value}"
}

proc emit_private(value: Str) -> Result[Unit] {
  print ${label_private(value)}
}

## Exposes the package name.
export let name = prefix

## Renders a package label.
export pure label(value: Str) -> Str {
  return label_private(value)
}

## Emits the package build value.
export proc build(value: Str) -> Result[Unit] {
  emit_private(value)?
}
""")?

  let module_env = {XSH_MODULE_PATH: root.display()}
  let success = test.run_script(
    ctx,
    f"""
type DynamicPackage = module {{
  export let name: Str
  export pure label(value: Str) -> Str
  export proc build(value: Str) -> Result[Unit]
}}
let checked = module.load(p"{package}")?.require(DynamicPackage)?
let name: Str = checked.name
let rendered: Str = checked.label(name)
print ${{rendered}}
checked.build("built")?
""",
    [],
    module_env,
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = success
    assert assertion_condition, assertion_message
  }
  assert success.stdout == """demo-demo
demo-built
"""

  let private = test.run_script(
    ctx,
    f"""let loaded = module.load(p"{package}")?
let value = loaded.prefix
""",
    [],
    module_env,
  )?
  assert private.status == 3
  assert "missing-field" in private.stderr
  assert "prefix" in private.stderr

  let mismatch = test.run_script(
    ctx,
    f"""
type BadPackage = module {{
  export proc build(path: Path) -> Result[Unit]
}}
let loaded = module.load(p"{package}")?
match loaded.require(BadPackage) {{
  Err(error) => test.error_kind(error, "schema")?
  Ok(_) => test.fail("incompatible contract succeeded")?
}}
""",
    [],
    module_env,
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = mismatch
    assert assertion_condition, assertion_message
  }
}

test test_module_load_rejects_undocumented_export { |ctx|
  let root = test.temp_dir(ctx, name: "undocumented-module")?
  let plugin_path = fp"{root}/undocumented.xsh"
  fs.write(
    plugin_path,
    """export let name = "undocumented"
""",
  )?

  let output = test.run_script(
    ctx,
    f"""let _ = module.load(p"{plugin_path}")?
""",
  )?

  assert ! output.success
  assert "undocumented exports" in output.stderr
}

test test_module_load_rejects_forbidden_top_level_forms { |ctx|
  let root = test.temp_dir(ctx, name: "module-top-level")?
  for fixture in [
    {
      name: "bad-var",
      source: """##! Invalid dynamic module fixture.
var count = 1
export let name = "bad"
""",
    },
    {
      name: "bad-command",
      source: """##! Invalid dynamic module fixture.
print bad
export let name = "bad"
""",
    },
  ] {
    let module_path = fp"{root}/{fixture.name}.xsh"
    fs.write(module_path, fixture.source)?
    let output = test.run_script(
      ctx,
      f"""let _ = module.load(p"{module_path}")?
""",
    )?
    assert output.status == 3
    assert "module-check" in output.stderr
    assert "check.module-top-level" in output.stderr
    assert module_path.name() in output.stderr
  }

  let hook = fp"{root}/signal-hook.xsh"
  fs.write(
    hook,
    """on SIGINT [] {
}
""",
  )?
  let output = test.run_script(
    ctx,
    f"""let _ = module.load(p"{hook}")?
""",
  )?
  assert output.status == 3
  assert "module-check" in output.stderr
  assert "check.signal-hook-module" in output.stderr
  assert hook.name() in output.stderr
}

test test_static_and_loaded_modules_reject_the_same_contract_mismatches { |ctx|
  let root = test.temp_dir(ctx, name: "module-contract-mismatches")?
  let optional_path = fp"{root}/bad_optional.xsh"
  let effect_path = fp"{root}/bad_effect.xsh"
  optional_path.write("""
##! Module with an incompatible optional export.

## Deliberately not the contract's string type.
export let description: Int = 1
""")?
  effect_path.write("""
##! Module with an implementation effect outside its contract.

## Deliberately requires an extra process capability.
export proc execute() [fs, process, error] -> Result[Unit] {
  return Ok()
}
""")?

  let optional_contract = """\ntype Plugin = module {
  export optional let description: Str
}
"""
  let effect_contract = """\ntype Runner = module {
  export proc execute() [fs, error] -> Result[Unit]
}
"""

  for source in [
    f"""{optional_contract}
use bad_optional
let _: Plugin = bad_optional
""",
    f"""{effect_contract}
use bad_effect
let _: Runner = bad_effect
""",
    f"""{optional_contract}
let _ = module.load(p"{optional_path}")?.require(Plugin)?
""",
    f"""{effect_contract}
let _ = module.load(p"{effect_path}")?.require(Runner)?
""",
  ] {
    let result = test.run_script(ctx, source, [], {XSH_MODULE_PATH: root})?
    {
      let assertion_condition = ! result.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
  }
}

test test_static_module_namespace_satisfies_the_same_contract { |ctx|
  let root = test.temp_dir(ctx, name: "static-module-contract")?
  fp"{root}/runner.xsh".write("""
##! Static runner fixture.
## Writes the fixture marker.
export proc execute(root: Path) [fs, error] -> Result[Unit] {
  fp"{root}/out.txt".write("static")?
}
""")?

  let result = test.run_script(
    ctx,
    f"""
type Runner = module {{
  export proc execute(root: Path) [fs, error] -> Result[Unit]
}}

use runner

proc main() [fs, error] -> Result[Unit] {{
  let checked: Runner = runner
  checked.execute(p"{root}")?
}}

main()?
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  assert fp"{root}/out.txt".read_text()? == "static"
}

test test_same_basename_modules_keep_separate_top_level_bindings { |ctx|
  let root = test.temp_dir(ctx, name: "same-basename-modules")?
  fp"{root}/alpha".mkdir()?
  fp"{root}/beta".mkdir()?
  fp"{root}/alpha/proof.xsh".write("""
##! First proof module.
let numbers = [2, 3]

## Sums the first module's numbers.
export pure sum_numbers() -> Int {
  var total = 0
  for number in numbers {
    total += number
  }
  return total
}
""")?
  fp"{root}/beta/proof.xsh".write("""
##! Second proof module.
let words = ["one", "two", "three"]

## Counts the second module's words.
export pure count_words() -> Int {
  var total = 0
  for word in words {
    total += 1
  }
  return total
}
""")?

  let result = test.run_script(
    ctx,
    r"""
use alpha.proof as alpha
use beta.proof as beta
print ${alpha.sum_numbers()}
print ${beta.count_words()}
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  assert result.stdout == """5
3
"""
}

test test_imported_local_args_shadows_predeclared_script_arguments { |ctx|
  let root = test.temp_dir(ctx, name: "imported-if-list")?
  fp"{root}/selector.xsh".write("""
##! Selects a list in an imported function.
## Returns the unchanged argument list when no separator is present.
export pure select(argv: List[Str]) -> List[Str] {
  let args = if argv.len() > 0 and argv[0] == "--" { [] } else { argv }
  return args
}
""")?

  let result = test.run_script(
    ctx,
    r"""
use selector
print ${selector.select(["unknown"]).len()}
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  assert result.stdout == """1
"""
}

test test_imported_error_constructor_named_fields_preserve_identity_and_order { |ctx|
  let root = test.temp_dir(ctx)?
  fp"{root}/helper.xsh".write(r"""
##! Imported error constructor fixture.
## A checked failure with two named fields.
export error HelperError = Failed(detail: Str, code: Int) : Temporary
## Constructs a failure inside the defining module.
export pure failure() -> Result[Unit] {
  Err(HelperError.Failed(detail: "loaded", code: 9))
}
""")?
  let result = test.run_script(
    ctx,
    r"""
use helper
var order = 0
let failure = helper.HelperError.Failed(
  code: { order = order * 10 + 1; 7 },
  detail: { order = order * 10 + 2; "failed" },
)
match failure {
  helper.HelperError.Failed {detail, code} => print $detail $code $order
  _ => print "wrong family"
}
let dynamic: Any = failure
match dynamic {
  _ is helper.Temporary => print "temporary"
  _ => print "wrong facet"
}
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  assert result.stdout == """failed 7 12
temporary
"""
  let aliased = test.run_script(
    ctx,
    r"""
use helper as h
let failure = h.HelperError.Failed(detail: "aliased", code: 8)
match failure {
  h.HelperError.Failed {detail, code} => print $detail $code
  _ => print "wrong family"
}
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = aliased
    assert assertion_condition, assertion_message
  }
  assert aliased.stdout == """aliased 8
"""
  let loaded = test.run_script(
    ctx,
    f"""
type FailureProvider = module {{
  export pure failure() -> Result[Unit]
}}
let provider = module.load(p"{root}/helper.xsh")?.require(FailureProvider)?
print ${{provider.failure() is Err(_)}}
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = loaded
    assert assertion_condition, assertion_message
  }
  assert loaded.stdout == """true
"""
}

test test_static_module_exports_bind_one_namespace { |ctx|
  let root = test.temp_dir(ctx, name: "module-namespace")?
  fp"{root}/helper.xsh".write("""
##! Namespace-only static module fixture.

## A value export.
export let value: Str = "helper"
## An effectful callable export.
export proc execute() [error] -> Result[Unit] {
  return Ok()
}
## A pure callable export.
export pure render(value: Str) -> Str {
  return value.upper()
}
## A stream export.
export stream numbers() [] -> Stream[Int] {
  yield 1
}
## A tagged union export.
export enum State { Ready, Stopped(Str) }
## An error family export with an error facet.
export error HelperError = Failed(detail: Str) : Temporary
""")?

  let positive = test.run_script(
    ctx,
    """
use helper

proc check_stream() [] -> Unit {
  for value in helper.numbers() {
    let _: Int = value
  }
}

proc main() [error] -> Result[Unit] {
  let _: helper.State = helper.Ready
  let _: helper.State = helper.Stopped("stopped")
  let _: helper.HelperError = helper.HelperError.Failed(detail: "failed")
  helper.execute()?
  print \$helper.value
  print \${helper.render("ok")}
  let dynamic: Any = helper.HelperError.Failed(detail: "failed")
  match dynamic {
    _ is helper.Temporary => print "temporary"
    _ => print "missing"
  }
}


main()?
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = positive
    assert assertion_condition, assertion_message
  }

  for source in [
    """use helper
execute()
""",
    """use helper
let _: State = helper.Ready
""",
    """use helper
Stopped("stopped")
""",
    """use helper
HelperError.Failed(detail: "failed")
""",
    """use helper
match helper.HelperError.Failed(detail: "failed") {
  _ is Temporary => print "bare facet"
  _ => print "fallback"
}
""",
  ] {
    let result = test.run_script(ctx, source, [], {XSH_MODULE_PATH: root})?
    {
      let assertion_condition = ! result.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
  }

  let aliased = test.run_script(
    ctx,
    """
use helper as h

proc main() [error] -> Result[Unit] {
  let _: h.State = h.Ready
  let _: h.HelperError = h.HelperError.Failed(detail: "failed")
  h.execute()?
  print h.render("ok")
}

main()?
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = aliased
    assert assertion_condition, assertion_message
  }

  for source in [
    """use helper as h
helper.execute()
""",
    """use helper as h
execute()
""",
    """use helper as h
let _: helper.State = h.Ready
""",
    """use helper as h
let _: State = h.Ready
""",
    """use helper as h
HelperError.Failed(detail: "failed")
""",
  ] {
    let result = test.run_script(ctx, source, [], {XSH_MODULE_PATH: root})?
    {
      let assertion_condition = ! result.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
  }
}

test test_qualified_module_functions_remain_callable_as_values { |ctx|
  let root = test.temp_dir(ctx, name: "qualified-module-functions")?
  fp"{root}/package.xsh".write(r"""
##! Qualified values fixture module.
## Exposes a typed package value and operations.
## Public package type.
export type Package = {name: Str}

## Labels a package.
export pure label(pkg: Package) -> Str {
  return f"pkg:{pkg.name}"
}

## Shows a package label.
export proc show(pkg: Package) -> Result[Unit] {
  print ${label(pkg)}
}

## Public package value.
export let pkg: Package = {name: "demo"}
""")?

  let output = test.run_script(
    ctx,
    r"""
use package as p
let labeler = p.label
let shower = p.show
let pkg: p.Package = p.pkg
print ${labeler.call(pkg)}
shower.call(pkg)?
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """pkg:demo
pkg:demo
"""
}

test test_qualified_record_fields_pass_to_effectful_module_proc { |ctx|
  let root = test.temp_dir(ctx, name: "qualified-module-record")?
  fp"{root}/target.xsh".write(r"""
##! Target fixture module.
## Defines the target policy record used by lifecycle contexts.
## CPU feature policy nested in a target.
export type Cpu = {feature: Str}

## Public target policy.
export type Target = {triple: Str, cpu: Cpu}

## Selects the fixture target policy.
export pure select() -> Target {
  return {triple: "x86_64-unknown-linux-musl", cpu: {feature: "crt-static"}}
}
""")?
  fp"{root}/lifecycle.xsh".write(r"""
##! Lifecycle fixture module consuming contexts composed from the target policy record.
use target as targets

## Public lifecycle context.
export type Context = {root: Path, target: targets.Target}

## Normalizes the selected target policy.
export proc normalize(context: Context) [io] -> Result[Unit] {
  print ${context.root} ${context.target.triple} ${context.target.cpu.feature}
}
""")?

  let output = test.run_script(
    ctx,
    r"""
use lifecycle as l
use target as targets
let selected: targets.Target = targets.select()
let context: l.Context = {root: Path("workspace"), target: selected}
l.normalize(context)?
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """workspace x86_64-unknown-linux-musl crt-static
"""
}

test test_module_proc_call_preserves_runtime_cwd { |ctx|
  let root = test.temp_dir(ctx, name: "module-proc-cwd")?
  let src = fp"{root}/src"
  let out = fp"{root}/cwd.txt"
  let callee = fp"{root}/callee.xsh"
  src.mkdir()?
  callee.write(r"""
##! CWD writer module.
## Writes the active runtime working directory.
export proc write_cwd(out: Path) [fs, error] -> Result[Unit] {
  fs.write(out, fs.cwd()?.display())?
}
""")?
  fp"{root}/caller.xsh".write(f"""
##! CWD caller module.
type Writer = module {{
  export proc write_cwd(out: Path) [fs, error] -> Result[Unit]
}}

## Loads the writer and invokes it within the requested directory.
export proc invoke(src: Path, out: Path) [env, fs, error] -> Result[Unit] {{
  let module_exports = module.load(p"{callee}")?.require(Writer)?
  cd src {{
    module_exports.write_cwd(out)?
  }} ?
}}
""")?

  let output = test.run_script(
    ctx,
    f"""
use caller as c
c.invoke(p"{src}", p"{out}")?
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert out.read_text()? == src.display()
}

test test_module_path_resolves_nested_module_with_default_alias { |ctx|
  let root = test.temp_dir(ctx, name: "nested-module-path")?
  let lib = fp"{root}/lib"
  fp"{lib}/pm".mkdir()?
  fp"{lib}/pm/configure.xsh".write(r"""
##! Configure fixture module.
## Provides a package label.
## Labels a package.
export pure label(name: Str) -> Str {
  return f"configured {name}"
}
""")?

  let output = test.run_script(
    ctx,
    r"""
use pm.configure
print ${configure.label("pkgconf")}
""",
    [],
    {XSH_MODULE_PATH: lib},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """configured pkgconf
"""
  assert output.stderr == ""
}

test test_module_import_alias_trace_and_cycle { |ctx|
  let root = test.temp_dir(ctx, name: "module-imports")?
  fp"{root}/helper.xsh".write(r"""
##! Helper fixture module.
use package as p

let greeting = "hi"

pure line(name: Str) -> Str {
  return f"{greeting} {name}"
}

## Greets by name.
export proc greet(name: Str) -> Result[Unit] {
  print ${line(name)}
  return Ok()
}

## Shows a package name.
export proc show(pkg: p.Package) -> Result[Unit] {
  print ${line(pkg.name)}
  return Ok()
}
""")?
  fp"{root}/package.xsh".write(r"""
##! Package fixture module.
let secret = "hidden"
## Public package type.
export type Package = {name: Str, root: Path}
## Public package value.
export let pkg: Package = {name: "demo", root: Path("src")}
""")?

  let source = """\nuse helper
use package as p
helper.greet(\"world\")?
helper.greet(\"namespace\")?
helper.show(p.pkg)?
print \${p.pkg.name}
match p.get(\"Package\") {
  Err(error) => {
    test.error_kind(error, \"missing-field\")?
    print \"missing-field\"
  }
}
"""
  let module_env = {XSH_MODULE_PATH: root.display()}
  let output = test.run_script(ctx, source, [], module_env)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """hi world
hi namespace
hi demo
demo
missing-field
"""
  assert output.stderr == ""

  let traced = test.run_xsht_trace(ctx, source, ["--raw"], [], module_env)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = traced
    assert assertion_condition, assertion_message
  }
  assert "kind=pure.enter" in traced.stderr
  assert "greet" in traced.stderr

  fp"{root}/a.xsh".write("""##! Cycle fixture A.
## Public cycle value.
use b
export let value = 1
""")?
  fp"{root}/b.xsh".write("""##! Cycle fixture B.
## Public cycle value.
use a
export let value = 2
""")?
  let cycle = test.run_script(
    ctx,
    """use a
""",
    [],
    module_env,
  )?
  assert cycle.status == 2
  assert "parse.module-cycle" in cycle.stderr
}

test test_package_hook_module_calls_keep_dynamic_and_static_cwd { |ctx|
  let root = test.temp_dir(ctx, name: "package-hook-modules")?
  let dynamic_out = fp"{root}/dynamic-out"
  let static_src = fp"{root}/static-src"
  let static_out = fp"{root}/static-out"
  static_src.mkdir()?
  let package = fp"{root}/PKGBUILD.xsh"
  package.write(r"""
##! Package hook module.
## Exposes the package name.
export let name = "demo"

## Writes the package marker into the destination.
export proc build(dest: Path) [fs, error] -> Result[Unit] {
  fs.mkdir(dest)?
  fs.write(fp"{dest}/ok", f"{name}:{fs.cwd()?.name()}\n")?
}
""")?

  let dynamic = test.run_script(
    ctx,
    f"""
type Pkg = module {{
  export let name: Str
  export proc build(dest: Path) [fs, error] -> Result[Unit]
}}
let pkg = module.load(p"{package}")?.require(Pkg)?
pkg.build(p"{dynamic_out}")?
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = dynamic
    assert assertion_condition, assertion_message
  }
  assert fp"{dynamic_out}/ok".read_text()? == f"""demo:{fs.cwd()?.name()}
"""

  let static_output = test.run_script(
    ctx,
    r"""
use PKGBUILD
proc main(src: Path, dest: Path) [fs, process, env, error] -> Result[Unit] {
  fs.remove(dest, missing_ok: true)?
  fs.mkdir(dest)?
  cd src {
    PKGBUILD.build(dest)?
  } ?
}
main(@args)?
""",
    [static_src.display(), static_out.display()],
    {XSH_MODULE_PATH: root},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = static_output
    assert assertion_condition, assertion_message
  }
  assert fp"{static_out}/ok".read_text()? == """demo:static-src
"""
}

test test_stream_exports_are_namespace_members_not_module_contract_members { |ctx|
  let root = test.temp_dir(ctx, name: "module-stream-contract")?
  fp"{root}/stream_only.xsh".write("""
export stream numbers() [] -> Stream[Int] {
  yield 1
}
""")?

  let concrete_empty = test.run_script(
    ctx,
    """
type Runner = module {
  export proc run() [error] -> Result[Unit]
}

use stream_only
let _: Runner = stream_only
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let assertion_condition = ! concrete_empty.success
    let assertion_message = concrete_empty.stderr
    assert assertion_condition, assertion_message
  }

  let stream_contract = test.run_script(
    ctx,
    """
type Invalid = module {
  export stream numbers() [] -> Stream[Int]
}
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  {
    let assertion_condition = ! stream_contract.success
    let assertion_message = stream_contract.stderr
    assert assertion_condition, assertion_message
  }
}

test test_module_load_reports_module_path_and_parse_cause { |ctx|
  let root = test.temp_dir(ctx, name: "unparsable-module")?
  let broken = fp"{root}/broken.xsh"
  fs.write(
    broken,
    """## Broken export.
export pure answer() -> Int {
  1 +
""",
  )?

  let output = test.run_script(
    ctx,
    f"""let _ = module.load(p"{broken}")?
""",
  )?

  assert ! output.success
  assert f"module `{broken}` failed to parse" in output.stderr, output.stderr
  assert f"{broken}:4:1: parse.expected-expression" in output.stderr, output.stderr
}

test test_module_load_resolves_uses_with_configured_test_module_roots { |ctx|
  let project = test.temp_dir(ctx, name: "module-roots-project")?
  fp"{project}/lib/shared".mkdir()?
  fp"{project}/plugins".mkdir()?
  fp"{project}/tests".mkdir()?
  fp"{project}/xsht-config.ini".write("""module_path = lib
""")?
  fp"{project}/lib/shared/answers.xsh".write("""##! Shared answers.

## The shared answer.
export pure answer() -> Int {
  42
}
""")?
  fp"{project}/plugins/plugin.xsh".write("""##! A plugin importing through a configured root.
use shared.answers as answers

## The answer resolved through the configured root.
export let value: Int = answers.answer()
""")?
  fp"{project}/tests/loader.xsh".write("""type AnswerPlugin = module {
  export let value: Int
}

test loads_plugin_with_configured_roots {
  let plugin = module.load(p"plugins/plugin.xsh")?.require(AnswerPlugin)?
  assert plugin.value == 42
}
""")?

  cd project {
    let output = run.capture --text "xsht" test tests/loader.xsh ?
    assert output.status.exited_with(0), f"{output.stdout}{output.stderr}"
  } ?
}

test test_spawned_xsh_children_append_configured_test_module_roots { |ctx|
  let project = test.temp_dir(ctx, name: "child-module-roots-project")?
  let inherited = test.temp_dir(ctx, name: "child-module-roots-inherited")?
  fp"{project}/lib/shared".mkdir()?
  fp"{project}/tests".mkdir()?
  fp"{inherited}/extra".mkdir()?
  fp"{project}/xsht-config.ini".write("""module_path = lib
""")?
  fp"{project}/lib/shared/answers.xsh".write("""##! Shared answers.

## The shared answer.
export pure answer() -> Int {
  42
}
""")?
  fp"{inherited}/extra/greeting.xsh".write("""##! An inherited greeting.

## The greeting word.
export pure word() -> Str {
  "hi"
}
""")?
  fp"{project}/tests/children.xsh".write(
    r"""const child_source = "use shared.answers as answers\nuse extra.greeting as greeting\nprint greeting.word() answers.answer()\n"

test run_script_child_finds_configured_roots { |ctx|
  let output = test.run_script(ctx, child_source)?
  assert output.success, output.stderr
  assert output.stdout == "hi 42\n", output.stdout
}

test plain_run_child_finds_configured_roots { |ctx|
  let script = fp"{test.temp_dir(ctx, name: "plain-run")?}/child.xsh"
  script.write(child_source)?
  let output = run.capture --text "xsh" $script ?
  assert output.status.exited_with(0), output.stderr
  assert output.stdout == "hi 42\n", output.stdout
}
""",
  )?

  let inherited_root = inherited.display()
  env XSH_MODULE_PATH=$inherited_root {
    cd project {
      let output = run.capture --text "xsht" test tests/children.xsh ?
      assert output.status.exited_with(0), f"{output.stdout}{output.stderr}"
      assert "2 passed" in output.stdout, output.stdout
    } ?
  }
}

# `use a.b` binds `b`; the lint drops an alias that only repeats it, and the
# program means the same afterwards.
test test_redundant_use_alias_is_reported_and_fixed { |ctx|
  let root = test.temp_dir(ctx, name: "redundant-use-alias")?
  fp"{root}/checks".mkdir()?
  fp"{root}/checks/disk.xsh".write("""##! Disk checks.

## The usage threshold.
export pure threshold() -> Int {
  90
}
""")?
  let script = fp"{root}/main.xsh"
  script.write(r"""use checks.disk as disk
use checks.disk as usage

print ${disk.threshold()} ${usage.threshold()}
""")?

  let before = run.capture --text "xsh" $script ?
  assert before.status.exited_with(0), before.stderr

  let reported = run.capture --text "xsht" lint --only lint.redundant-use-alias $script ?
  assert "lint.redundant-use-alias" in reported.stderr, reported.stderr
  assert "`as disk` repeats the name this `use` already binds" in reported.stderr, reported.stderr
  assert "as usage" not in reported.stderr, reported.stderr

  let fixed = run.capture --text "xsht" lint --fix --only lint.redundant-use-alias $script ?
  assert fixed.status.exited_with(0), fixed.stderr
  assert script.read_text()? == r"""use checks.disk
use checks.disk as usage

print ${disk.threshold()} ${usage.threshold()}
"""

  let after = run.capture --text "xsh" $script ?
  assert after.status.exited_with(0), after.stderr
  assert after.stdout == before.stdout
  assert after.stdout == "90 90\n", after.stdout
}
