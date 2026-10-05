# A `nominal type` is a record schema whose name is an identity of its own. A
# value has the type only from the constructor or from `.require(Name)`; a
# record with the same fields is not one. Nothing else about the record
# changes: its fields are read, spread, matched, encoded, and compared as the
# record it is.

nominal type Package = {id: Str, version: Str}

type Plain = {id: Str, version: Str}

type Shipment = {pkg: Package, count: Int}

pure label(pkg: Package) -> Str {
  f"{pkg.id}-{pkg.version}"
}

pure plain_label(pkg: Plain) -> Str {
  f"{pkg.id}:{pkg.version}"
}

pure bump(pkg: Package, version: Str) -> Package {
  Package(id: pkg.id, version:)
}

pure describe(item: Union[Package, Int]) -> Str {
  match item {
    pkg is Package => label(pkg)
    total is Int => f"{total}"
  }
}

pure first_label(packages: List[Package]) -> Str {
  label(packages[0])
}

pure validated(value: Any) -> Result[Package] {
  value.require()
}

test test_constructor_and_require_produce_the_nominal_type {
  let built = Package(id: "a", version: "1")
  assert label(built) == "a-1"
  assert label(bump(built, "3")) == "a-3"

  # `.require` is the conversion, from a dynamic value and from a record
  # whose type is already known.
  let raw: Any = {id: "c", version: "4"}
  let converted = raw.require(Package)?
  assert label(converted) == "c-4"
  let plain = Plain(id: "d", version: "5")
  let branded = plain.require(Package)?
  assert label(branded) == "d-5"
  assert label(validated(raw)?) == "c-4"
  assert validated({id: "e"}) is Err(_)
  assert validated("text") is Err(_)

  let decoded = json.decode("""{"id": "f", "version": "6"}""")?.require(Package)?
  assert label(decoded) == "f-6"
  let listed = json.decode("""[{"id": "g", "version": "7"}]""")?.require(List[Package])?
  assert first_label(listed) == "g-7"
}

test test_nominal_value_is_read_as_its_record {
  let built = Package(id: "a", version: "1")
  assert built.id == "a"
  # It fits its structural base, here and inside a collection.
  assert plain_label(built) == "a:1"
  let plain: Plain = built
  assert plain.version == "1"
  let listed: List[Plain] = [built, bump(built, "2")]
  assert listed.len() == 2

  # A spread copies fields; the result is an ordinary record.
  let bumped = {...built, version: "2"}
  assert plain_label(bumped) == "a:2"
  let rebuilt = bumped.require(Package)?
  assert label(rebuilt) == "a-2"

  # Encoding, equality, destructuring, and record patterns read the record.
  assert json.encode(built)? == """{"id":"a","version":"1"}"""
  assert built == Package(id: "a", version: "1")
  assert built != bump(built, "2")
  let {id, version} = built
  assert f"{id} {version}" == "a 1"
  let matched = match built {
    {id: "a", ..} => "matched",
    else => "unmatched",
  }
  assert matched == "matched"

  let shipment = Shipment(pkg: built, count: 2)
  assert label(shipment.pkg) == "a-1"
}

type Catalog = {latest: Package, all: List[Package], named: Map[Package], maybe: Package?}

const FALLBACK: Package = Package(id: "none", version: "0")

# An unannotated return keeps the nominal type.
pure newest(catalog: Catalog) {
  catalog.latest
}

proc pick(packages: List[Package], id: Str) -> Result[Package] {
  for pkg in packages {
    return Ok(pkg) when pkg.id == id
  }

  fail f"no package {id}"
}

test test_nominal_values_in_collections_optionals_and_constants {
  let built = Package(id: "a", version: "1")
  var current = built
  current = bump(built, "2")
  let catalog = Catalog(latest: current, all: [built, current], named: {a: built}, maybe: null)
  assert label(newest(catalog)) == "a-2"
  assert pick(catalog.all, "a")?.version == "1"
  assert pick(catalog.all, "z") is Err(_)
  assert [pkg.version for pkg in catalog.all] == ["1", "2"]
  assert (catalog.all |> map .version |> collect()) == ["1", "2"]
  assert catalog.maybe?.id == null
  let present: Package? = if catalog.all.len() > 1 { built } else { null }
  assert present?.id == "a"
  assert label(present ?? FALLBACK) == "a-1"
  assert label(catalog.maybe ?? FALLBACK) == "none-0"
  assert built.keys() == ["id", "version"]
}

test test_nominal_member_of_a_union_is_narrowed {
  let built = Package(id: "a", version: "1")
  assert describe(built) == "a-1"
  assert describe(3) == "3"
  let item: Union[Package, Int] = built
  assert item is Package
  let number: Union[Package, Int] = 4
  assert ! (number is Package)
}

proc check_errors(ctx: TestContext, source: Str) [fs, process, env, error] -> Result[Str] {
  let output = test.run_script(ctx, source)?
  assert output.status == 2, f"{output.stdout}{output.stderr}"
  assert output.stdout == "", output.stdout
  output.stderr
}

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

test test_record_with_the_same_fields_is_not_the_nominal_type { |ctx|
  let stderr = check_errors(
    ctx,
    r"""nominal type Package = {id: Str, version: Str}
nominal type Release = {id: Str, version: Str}
type Plain = {id: Str, version: Str}

pure consume_package(pkg: Package) -> Str {
  pkg.id
}

pure forged(id: Str, version: Str) -> Package {
  {id, version}
}

let id = "a"
let version = "1"
let plain: Plain = {id, version}
let built = Package(id:, version:)
print consume_package({id, version})
print consume_package(plain)
let annotated: Package = {id, version}
let other: Release = built
let listed: List[Package] = [plain]
print ${annotated.id} ${other.id} ${listed.len()} ${forged(id, version).id}
""",
  )?
  assert count(stderr, "err[check.type-mismatch]") == 6, stderr
  assert count(stderr, "err[") == 6, stderr
  assert "expected Package, found Record" in stderr, stderr
  assert "expected Release, found Package" in stderr, stderr
  assert count(
    stderr,
    "build the value with the constructor `Package(...)`, or convert a record with `.require(Package)?`",
  ) == 5, stderr
}

# The identity is not in the value, so a runtime test could only compare
# fields. The checker allows the test where the static type already vouches.
test test_nominal_type_is_not_a_runtime_test { |ctx|
  let stderr = check_errors(
    ctx,
    r"""nominal type Package = {id: Str, version: Str}
type Plain = {id: Str, version: Str}

let plain: Plain = {id: "a", version: "1"}
let raw: Any = plain
let packages: Any = [plain]
print ${raw is Package}
print ${plain is Package}
print ${packages is List[Package]}
match raw {
  pkg is Package => print ${pkg.id}
  _ => print "other"
}
""",
  )?
  assert count(stderr, "err[check.pattern-type]") == 4, stderr
  assert count(stderr, "err[") == 4, stderr
  assert "`Package` cannot be tested on a value of type Any: `Package` is a nominal type, and a value does not carry that identity at run time" in stderr, stderr
  assert "convert the value with `.require(Package)?`" in stderr, stderr
}

test test_nominal_declaration_and_unions_are_well_formed { |ctx|
  let stderr = check_errors(
    ctx,
    r"""nominal type Package = {id: Str, version: Str}
nominal type Release = {id: Str, version: Str}
nominal type Alias = Str
nominal type Box[T] = {value: T}
type Plain = {id: Str, version: Str}
type Wider = {id: Str, version: Str, arch: Str}
type Same = Union[Package, Plain]
type Wide = Union[Package, Wider]
type Twins = Union[Package, Release]

let same: Same? = null
let wide: Wide? = null
let twins: Twins? = null
print ${same == null} ${wide == null} ${twins == null}
""",
  )?
  assert count(stderr, "err[check.schema]") == 2, stderr
  assert "a `Record` has the fields of the nominal type `Package`" in stderr, stderr
  assert "`nominal type Alias` must be a record schema without type parameters" in stderr, stderr
  assert "`nominal type Box` must be a record schema without type parameters" in stderr, stderr
  assert "every `Package` already fits the member" in stderr, stderr
  assert "a `Release` has the fields of the nominal type `Package`, and nothing at run time tells them apart" in stderr, stderr
}

# Two modules may each declare a nominal type of one name and shape; they are
# different types, and an exported one is constructed through its module.
test test_nominal_identity_belongs_to_the_declaring_module { |ctx|
  let root = test.temp_dir(ctx, name: "nominal-modules")?
  fp"{root}/registry.xsh".write(r"""
##! A package registry.
## A package the registry has resolved.
export nominal type Package = {id: Str, version: Str}

## Resolves a package by name.
export pure resolve(id: Str) -> Package {
  Package(id:, version: "1")
}

## The name a resolved package is installed under.
export pure label(pkg: Package) -> Str {
  f"{pkg.id}-{pkg.version}"
}
""")
  let accepted = test.run_xsh(
    ctx,
    r"""use registry

nominal type Package = {id: Str, version: Str}

pure local_label(pkg: Package) -> Str {
  f"local {pkg.id}"
}

let resolved = registry.resolve("a")
print registry.label(resolved)
print registry.label(registry.Package(id: "b", version: "2"))
let kept: registry.Package = resolved
print ${kept.version}
print local_label(Package(id: "c", version: "3"))
print local_label(resolved.require(Package)?)
""",
    env: {XSH_MODULE_PATH: root},
  )?
  assert accepted.success, accepted.stderr
  assert accepted.stdout == "a-1\nb-2\n1\nlocal c\nlocal a\n", accepted.stdout

  let rejected = test.run_xsh(
    ctx,
    r"""use registry

nominal type Package = {id: Str, version: Str}

let local = Package(id: "c", version: "3")
print registry.label(local)
print registry.label({id: "d", version: "4"})
""",
    env: {XSH_MODULE_PATH: root},
  )?
  assert rejected.status == 2, rejected.stderr
  assert count(rejected.stderr, "err[check.type-mismatch]") == 2, rejected.stderr
}

# A dynamic call checks its arguments where it runs, as `.require` does: the
# record's fields are all the parameter can be held to.
test test_dynamic_call_holds_a_nominal_parameter_to_its_fields { |ctx|
  let output = test.expect(
    ctx,
    r"""nominal type Package = {id: Str, version: Str}

pure label(pkg: Package) -> Str {
  f"{pkg.id}-{pkg.version}"
}

proc call_dynamic(handle: Pure, argument: Any) -> Result[Str] {
  handle.call(argument).require(Str)
}

print call_dynamic(label, {id: "a", version: "1"})?
print call_dynamic(label, {id: "a"})?
""",
    status: 3,
    stderr: ["expected Package, found Record"],
  )?
  assert output.stdout == "a-1\n", output.stdout
}
