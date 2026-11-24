type Labeled = module {
  export pure reported() -> Str
}

type Versioned = module {
  export pure version() -> Int
}

type Reaching = module {
  export pure doubled(value: Int) -> Int
  export pure tripled(values: List[Int]) -> List[Int]
  export pure quadrupled(value: Int) -> Int
}

type Aliasing = module {
  export pure quadrupled(value: Int) -> Int
}

type Escaping = module {
  export pure through_namespace(value: Int) -> Int
}

const LABELED_SHARED = """##! Shared state.

let label = p"@ROOT@/label.txt".read_text()?

## The label the importing program read when it initialized this module.
export pure current() -> Str { label }

## A helper no importer reaches.
export pure unused() -> Str { "unused" }
"""

const LABELED_IMPORTER = """##! Importer.
use shared

## The label this program's import read.
export pure reported() -> Str { shared.current() }
"""

const VERSIONED_IMPORTER = """##! Importer.
use shared

## The version of the shared module this program prepared.
export pure version() -> Int { shared.version() }
"""

const REACHING_SHARED = """##! Shared arithmetic.

## Doubles through a private helper.
export pure double(value: Int) -> Int { twice(value) }

pure twice(value: Int) -> Int { value * 2 }

## Applies the private helper as a value.
export pure quadruple(value: Int) -> Int {
  let step = twice
  step(step(value))
}

## Triples a value.
export pure triple(value: Int) -> Int { value * 3 }

## Halves a value.
export pure halve(value: Int) -> Int { value / 2 }

## A helper no importer reaches.
export pure unused() -> Int { 0 }

## The identity of multiplication.
export let unit = 1
"""

const REACHING_IMPORTER = """##! Importer.
use shared as arithmetic

## Doubles through the shared module.
export pure doubled(value: Int) -> Int { arithmetic.double(value) }

## Triples every value.
export pure tripled(values: List[Int]) -> List[Int] { [arithmetic.triple(value) for value in values] }

## Quadruples through the shared module.
export pure quadrupled(value: Int) -> Int { arithmetic.quadruple(value) }
"""

const ALIASING_IMPORTER = """##! Importer.
use shared

let quadrupler = shared.quadruple

## Quadruples through an alias of the shared function.
export pure quadrupled(value: Int) -> Int { quadrupler(value) }
"""

const VALUED_IMPORTER = """##! Importer.
use shared

## Quadruples and scales by the shared unit value.
export pure quadrupled(value: Int) -> Int { shared.quadruple(value) * shared.unit }
"""

const ESCAPING_IMPORTER = """##! Importer.
use shared

## Halves through the namespace held as a value.
export pure through_namespace(value: Int) -> Int {
  let tools = shared
  tools.halve(value)
}
"""

const FAILING_SHARED = """##! Shared checks.

## Fails for small values.
export proc checked(value: Int) -> Int {
  assert value > 1, "value too small"
  value
}

## A helper no importer reaches.
export pure unused() -> Int { 0 }
"""

const FAILING_IMPORTER = """##! Importer.
use shared

## Runs the shared check.
export proc execute() -> Int { shared.checked(1) }
"""

const BROKEN_SHARED = """##! Shared module with a checking error.

## Returns one.
export pure fine() -> Int { 1 }

## No importer reaches this declaration.
export pure broken() -> Int { "text" }
"""

const FINE_IMPORTER = """##! Importer.
use shared

## The shared value.
export pure fine() -> Int { shared.fine() }
"""

const FAILING_MAIN = """type Runner = module {
  export proc execute() -> Int
}
let loaded = module.load(p"@ROOT@/loaded.xsh")?.require(Runner)?
print \${loaded.execute()}
"""

proc write_module(root: Path, name: Str, text: Str) [fs, error] {
  fp"{root}/{name}.xsh".write(text.replace("@ROOT@", with: root.display()))
}

test test_loads_sharing_an_import_keep_their_own_initialization { |ctx|
  let root = test.temp_dir(ctx, name: "shared-labels")?
  write_module(root, "shared", LABELED_SHARED)
  write_module(root, "first", LABELED_IMPORTER)
  write_module(root, "second", LABELED_IMPORTER)
  fp"{root}/label.txt".write("one")
  let first = module.load(fp"{root}/first.xsh")?.require(Labeled)?
  fp"{root}/label.txt".write("two")
  let second = module.load(fp"{root}/second.xsh")?.require(Labeled)?
  assert first.reported() == "one"
  assert second.reported() == "two"
}

test test_changed_import_is_prepared_again { |ctx|
  let root = test.temp_dir(ctx, name: "changed-import")?
  let shared = "##! Shared version.\n\n## The version this file declares.\nexport pure version() -> Int { @VERSION@ }\n"
  write_module(root, "shared", shared.replace("@VERSION@", with: "1"))
  write_module(root, "first", VERSIONED_IMPORTER)
  write_module(root, "second", VERSIONED_IMPORTER)
  let first = module.load(fp"{root}/first.xsh")?.require(Versioned)?
  assert first.version() == 1
  write_module(root, "shared", shared.replace("@VERSION@", with: "2"))
  let second = module.load(fp"{root}/second.xsh")?.require(Versioned)?
  assert second.version() == 2
  let reloaded = module.load(fp"{root}/first.xsh")?.require(Versioned)?
  assert reloaded.version() == 2
}

test test_every_reference_form_reaches_its_import { |ctx|
  let root = test.temp_dir(ctx, name: "reference-forms")?
  write_module(root, "shared", REACHING_SHARED)
  write_module(root, "loaded", REACHING_IMPORTER)
  write_module(root, "aliasing", ALIASING_IMPORTER)
  write_module(root, "valued", VALUED_IMPORTER)
  write_module(root, "escaping", ESCAPING_IMPORTER)
  let loaded = module.load(fp"{root}/loaded.xsh")?.require(Reaching)?
  assert loaded.doubled(4) == 8
  assert loaded.tripled([1, 2]) == [3, 6]
  assert loaded.quadrupled(3) == 12
  let aliasing = module.load(fp"{root}/aliasing.xsh")?.require(Aliasing)?
  assert aliasing.quadrupled(2) == 8
  let valued = module.load(fp"{root}/valued.xsh")?.require(Aliasing)?
  assert valued.quadrupled(5) == 20
  let escaping = module.load(fp"{root}/escaping.xsh")?.require(Escaping)?
  assert escaping.through_namespace(10) == 5
}

test test_failure_in_an_import_reports_its_location { |ctx|
  let root = test.temp_dir(ctx, name: "import-failure")?
  fp"{root}/xsht-config.ini".write("")
  write_module(root, "shared", FAILING_SHARED)
  write_module(root, "loaded", FAILING_IMPORTER)
  write_module(root, "main", FAILING_MAIN)
  let main = fp"{root}/main.xsh"
  let output = run.capture --text xsh $main
  assert ! output.status.exited_with(0)
  assert output.stdout == ""
  assert "value too small" in output.stderr, output.stderr
  assert "shared.xsh.checked" in output.stderr, output.stderr
}

test test_import_check_errors_fail_the_load { |ctx|
  let root = test.temp_dir(ctx, name: "import-check-error")?
  write_module(root, "shared", BROKEN_SHARED)
  write_module(root, "loaded", FINE_IMPORTER)
  match module.load(fp"{root}/loaded.xsh") {
    Ok(_) => test.fail("a module whose import does not check loaded")
    Err(error) => {
      assert "failed to check" in error.message, error.message
      assert "shared.xsh:7:" in error.message, error.message
    }
  }
}
