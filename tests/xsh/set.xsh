# `Set[T]` holds distinct elements of one key type in key order. Braces are
# a set where an entry is not a bare name or where a `Set[T]` is expected.

type Inventory = {name: Str, tags: Set[Str]}

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

pure both(left: Set[Str], right: Set[Str]) -> Set[Str] {
  left & right
}

pure tags_of(names: List[Str]) -> Set[Str] {
  {name.lower() for name in names if ! name.is_empty()}
}

test test_set_literals_hold_each_element_once_in_key_order {
  let words = {"pear", "apple", "pear", "fig"}
  assert words.len() == 3
  assert words.to_list() == ["apple", "fig", "pear"]
  assert {3, 1, 2, 1}.to_list() == [1, 2, 3]
  assert {true, false}.to_list() == [false, true]
  assert {2s, 500ms}.to_list() == [500ms, 2s]
  assert {"only",}.len() == 1

  let multiline = {
    "one",
    "two",
  }
  assert multiline == {"two", "one"}
}

test test_one_entry_that_is_not_a_bare_name_makes_braces_a_set {
  let first = "a"
  let second = "b"
  let mixed = {first, "b"}
  assert mixed == {"a", "b"}
  assert {first, second.upper()} == {"a", "B"}

  # Bare names alone are a record, and a set only where one is expected.
  let record = {first, second}
  assert record.first == "a"
  let expected: Set[Str] = {first, second}
  assert expected == mixed
  assert both({first, second}, {second}) == {"b",}

  # Where `Any` is expected the spelling alone decides.
  let dynamic: Any = {first, "b"}
  assert dynamic is Set[Str]
  let plain: Any = {first, second}
  assert ! (plain is Set[Str])
}

test test_set_operators_and_methods_return_new_sets {
  let left = {"a", "b", "c"}
  let right = {"b", "c", "d"}
  assert left | right == {"a", "b", "c", "d"}
  assert left & right == {"b", "c"}
  assert left - right == {"a",}
  assert left | right & {"d",} == {"a", "b", "c", "d"}
  assert "a" in left
  assert "d" not in left
  assert "d" in left | right

  let grown = left.add("z").add("a")
  assert grown.len() == 4
  assert left.len() == 3
  assert grown.remove("z") == left
  assert left.remove("missing") == left
  assert ! left.is_empty()

  let none: Set[Str] = set.empty()
  assert none.is_empty()
  assert none.len() == 0
  assert none | left == left
  let numbers: Set[Int] = set.from([3, 1, 3])
  assert numbers == {1, 3}
  assert [3, 1, 3].to_set() == numbers
}

test test_sets_are_sources_of_loops_comprehensions_and_pipelines {
  let words = {"pear", "apple", "fig"}
  var seen = ""
  for word in words {
    seen += word + ","
  }

  assert seen == "apple,fig,pear,"
  assert [word.byte_len() for word in words] == [5, 3, 4]
  assert {word.byte_len() for word in words if word != "fig"} == {4, 5}
  assert (words |> map .upper() |> collect()) == ["APPLE", "FIG", "PEAR"]
  assert tags_of(["B", "", "a", "b"]).to_list() == ["a", "b"]

  var total: Set[Int] = set.empty()
  for n in [5, 3, 5] {
    total = total.add(n)
  }

  assert total.to_list() == [3, 5]
}

test test_sets_cross_json_as_sorted_arrays {
  let inventory = Inventory(name: "core", tags: {"b", "a"})
  let text = json.encode(inventory)?
  assert text == r"""{"name":"core","tags":["a","b"]}"""
  let decoded = json.decode(text)?.require(Inventory)?
  assert decoded.tags == {"a", "b"}
  assert decoded == inventory

  let paths: Set[Path] = {"b/c", "a"}
  assert p"a" in paths
  assert json.decode("[2, 1]")?.require(Set[Int])? == {1, 2}

  # A validation boundary does not drop data: a repeated element fails.
  let repeated = json.decode(r"""["a", "a"]""")?.require(Set[Str])
  assert repeated is Err(_)
  let wrong = json.decode(r"""["a", 1]""")?.require(Set[Str])
  assert wrong is Err(_)
  let scalar = json.decode("3")?.require(Set[Int])
  assert scalar is Err(_)
}

test test_set_misuse_is_rejected_by_the_checker { |ctx|
  let output = test.expect(
    ctx,
    """let ratios = {1.5, 2.5}
let nested: Set[List[Str]] = set.empty()
let rows = [[1], [2]].to_set()
let words = {"a", "b"}
let ready = true | false
let joined = words & ["c"]
let mixed = {"a", 1}
let none: Set[Str] = {}
let one: Set[Str] = {"a"}
let first = words[0]
print \${ratios.len()} \${nested.len()} \${rows.len()} \$ready \${joined.len()}
print \${mixed.len()} \${none.len()} \${one.len()} \$first
""",
    status: 2,
  )?
  assert output.stdout == ""
  assert count(output.stderr, "err[check.set-element-type]") == 3, output.stderr
  assert count(output.stderr, "err[check.set-operator]") == 2, output.stderr
  assert count(output.stderr, "err[check.type-mismatch]") == 3, output.stderr
  assert count(output.stderr, "err[check.index-type]") == 1, output.stderr
  assert "the boolean operator is `or`" in output.stderr, output.stderr
  assert "the empty set is `set.empty()`" in output.stderr, output.stderr
  assert "a one-element set has a comma after its element" in output.stderr, output.stderr
}

test test_fields_and_elements_do_not_mix_in_one_literal { |ctx|
  let output = test.expect(
    ctx,
    """let name = "core"
let entry = {"kind", name: name}
print \${entry.len()}
""",
    status: 2,
  )?
  assert count(output.stderr, "err[parse.brace-literal-mixed]") == 1, output.stderr

  let doubled = test.expect(
    ctx,
    """let ready = true
if ready || ready { print "yes" }
""",
    status: 2,
  )?
  assert count(doubled.stderr, "err[parse.unsupported-boolean-operator]") == 1, doubled.stderr
}

# The formatter keeps the spellings that make braces a set.
test test_formatter_keeps_set_literals_sets { |ctx|
  let source = """let a = "x"
let one = {"only",}
let mixed = {a, "y"}
let squares = {n * n for n in [1, 2]}
let wide = {
  "alpha-alpha-alpha-alpha-alpha-alpha",
  "beta-beta-beta-beta-beta-beta-beta",
  "gamma-gamma-gamma-gamma-gamma-gamma",
}
print \${one.len()} \${mixed.len()} \${squares.len()} \${wide.len()}
"""
  let candidate = test.temp_file(ctx, name: "sets.xsh", contents: bytes.from_text(source))?
  let stable = run.capture --text "xsht" fmt --check $candidate ?
  assert stable.status.exited_with(0), stable.stdout + stable.stderr
  let output = test.expect(ctx, source, status: 0)?
  assert output.stdout == "1 2 2 3\n"
}
