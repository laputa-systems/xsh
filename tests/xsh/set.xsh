# `Set[T]` holds distinct elements of one key type in key order. Braces are
# a set where an entry is not a bare name or where a `Set[T]` is expected.

type Inventory = {name: Str, tags: Set[Str]}

const known_words = {"pear", "apple", "pear"}
const first_word = "fig"
const named_words: Set[Str] = {first_word, first_word}
const known_paths: Set[Path] = {"b/c", "a"}
const word_groups = {sizes: [{2, 1}], words: known_words}

type Search = {among: Set[Str] = known_words, word: Str}

pure is_known(word: Str, among: Set[Str] = known_words) -> Bool {
  word in among
}

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
  assert none.len() < 1
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

# A set literal over constants is a constant: prepared once, held in key
# order, and usable wherever a set is, including as a default.
test test_set_literals_over_constants_are_constants {
  const local_sizes: Set[Int] = {3, 1, 3}
  assert known_words.to_list() == ["apple", "pear"]
  assert named_words == {"fig",}
  assert p"a" in known_paths
  assert local_sizes.to_list() == [1, 3]
  assert word_groups.sizes[0] == {1, 2}
  assert word_groups.words == known_words

  assert is_known("pear")
  assert ! is_known("pear", among: named_words)
  assert Search(word: "x").among == known_words

  # A constant is a value: growing a copy leaves it as it was.
  var grown = known_words
  grown = grown.add("kiwi")
  assert grown.len() == 3
  assert (known_words | named_words).len() == 3
  assert known_words.len() == 2
}

test test_constant_sets_reject_what_is_not_constant_data { |ctx|
  let output = test.expect(
    ctx,
    """const ratios = {1.5, 2.5}
const wrong: Set[Int] = {"a", "b"}
const none: Set[Str] = set.empty()
const built = {"a".upper(), "b"}
const base = {"a", "b"}
const joined = base | {"c",}
print \${ratios.len()} \${wrong.len()} \${none.len()} \${built.len()} \${joined.len()}
""",
    status: 2,
  )?
  assert output.stdout == ""
  assert count(output.stderr, "err[check.set-element-type]") == 1, output.stderr
  assert count(output.stderr, "err[check.type-mismatch]") == 2, output.stderr
  assert count(output.stderr, "err[check.const]") == 4, output.stderr
}

test test_sets_cross_json_as_sorted_arrays {
  let inventory = Inventory("core", {"b", "a"})
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
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stdout + stable.stderr
  let output = test.expect(ctx, source, status: 0)?
  assert output.stdout == "1 2 2 3\n"
}

# After `{` or an entry's `,`, a `[` starts a computed map key, so a set
# element that begins with a list has the list in parentheses. Those
# parentheses are required, and the formatter writes them.
test test_a_set_element_that_begins_with_a_list_is_grouped { |ctx|
  let sizes = {([1, 2]).len(), 3, ([n for n in [4, 5, 6]]).len()}
  assert sizes == {2, 3}
  assert {([n, n]).len() + n for n in [1, 2]} == {3, 4}

  let source = """let sizes = {([1, 2].len()), 3}
let later = {7, ([1, 2, 3]).len()}
print \${sizes.len()} \${later.len()}
"""
  let output = test.expect(ctx, source, status: 0)?
  assert output.stdout == "2 2\n"
  let candidate = test.temp_file(ctx, name: "grouped.xsh", contents: bytes.from_text(source))?
  run.capture --text --accept=[0, 1] "xsht" fmt $candidate
  let formatted = candidate.read_text()?
  assert "let sizes = {([1, 2]).len(), 3}\n" in formatted, formatted
  assert "let later = {7, ([1, 2, 3]).len()}\n" in formatted, formatted
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stdout + stable.stderr

  # Parentheses that no entry needs are still redundant.
  let redundant = test.expect(
    ctx,
    """let sizes = {(1 + 2), 3}
print \${sizes.len()}
""",
    status: 2,
  )?
  assert count(redundant.stderr, "err[check.redundant-parens]") == 1, redundant.stderr
}

# The migration lint makes a local map of `true` a set in one fix, and the
# result is formatted, checks, and behaves the same.
test test_lint_makes_a_local_map_of_true_a_set { |ctx|
  let source = """proc show(words: List[Str]) [io] {
  var seen: Map[Bool] = {}
  var extra: Map[Str, Bool] = map.empty()
  for word in words {
    if word not in seen {
      seen[word] = true
      extra = extra.set(word.upper(), true)
    }
  }

  let kept: Map[Bool] = {a: true, b: false}
  print \${seen.len()} \${seen.keys().join(",")} \${extra.keys().join(",")} \${kept.len()}
}

show(["b", "a", "b"])
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "sets.xsh", contents: bytes.from_text(source))?

  let linted = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-set $candidate
  let report = linted.stdout + linted.stderr
  assert count(report, "warn[lint.prefer-set]") == 3, report
  assert count(report, "is a map used as a set") == 2, report
  assert count(report, "may be a set") == 1, report

  # Two fixes that overlap are applied one per pass. The advice has no fix
  # and stays reported.
  repeat 2 times {
    run.capture --text --accept=[0, 1] "xsht" lint --fix --only lint.prefer-set $candidate
  }

  let left = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-set $candidate
  assert count(left.stdout + left.stderr, "warn[lint.prefer-set]") == 1, left.stderr

  let rewritten = candidate.read_text()?
  assert "  var seen: Set[Str] = set.empty()\n" in rewritten, rewritten
  assert "  var extra: Set[Str] = set.empty()\n" in rewritten, rewritten
  assert "      seen = seen.add(word)\n" in rewritten, rewritten
  assert "      extra = extra.add(word.upper())\n" in rewritten, rewritten
  assert "seen.to_list().join" in rewritten, rewritten
  assert "  let kept: Map[Bool] = {a: true, b: false}\n" in rewritten, rewritten

  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
  let after = test.expect(ctx, rewritten, status: 0)?
  assert after.stdout == before.stdout
  assert after.stdout == "2 a,b A,B 2\n"
}

# The fix replaces the text from the declaration to the last use. A comment
# between the two is carried along, and the binding's name as a method or in
# a message is not a use of it, so the fix still applies.
test test_lint_fix_keeps_comments_and_reads_past_the_name_as_text { |ctx|
  let source = """proc show(row: Map[Int], words: List[Str]) [io] {
  var keys: Map[Bool] = {}
  var count = 0
  for word in words {
    # A repeated word is counted once.
    continue when word in keys

    if word in row.keys() {
      print "keys known"
    }

    keys[word] = true  # the first sighting
    count += 1
  }

  print \$count
}

show({a: 1}, ["b", "a", "b"])
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "commented.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text --accept=[0, 1] "xsht" lint --fix --only lint.prefer-set $candidate
  assert fixed.status.exited_with(0), fixed.stdout + fixed.stderr

  let rewritten = candidate.read_text()?
  assert "  var keys: Set[Str] = set.empty()\n" in rewritten, rewritten
  assert "    # A repeated word is counted once.\n" in rewritten, rewritten
  assert "    if word in row.keys() {\n" in rewritten, rewritten
  assert "    keys = keys.add(word)  # the first sighting\n" in rewritten, rewritten
  let after = test.expect(ctx, rewritten, status: 0)?
  assert after.stdout == before.stdout
  assert after.stdout == "keys known\n2\n"
}

# The `set` module builds sets and nothing else: `set.from` takes its element
# type from the list, `set.empty()` needs one from where it is written, and
# the functions that updated a map of `true` name the method that replaced
# them.
test test_set_module_constructs_sets_and_names_the_removed_functions { |ctx|
  let sizes = set.from([3, 1, 3])
  assert sizes == {1, 3}
  let paths: Set[Path] = set.from(["b", "a"])
  assert paths.to_list() == [p"a", p"b"]
  assert set.from(["x", "x"]).add("y").len() == 2

  let source = """var seen: Set[Str] = set.from(["a"])
seen = set.add(seen, "b")
seen = set.remove(seen, "a")
let none = set.empty()
let ratios = set.from([1.5])
let text = set.from("abc")
print \${seen.len()} \${none.len()} \${ratios.len()} \${text.len()}
"""
  let output = test.expect(ctx, source, status: 2)?
  assert output.stdout == ""
  assert count(output.stderr, "err[check.removed-set-function]") == 2, output.stderr
  assert "`set.add` was removed" in output.stderr, output.stderr
  assert "call the `.remove` method -> seen.remove(\"a\")" in output.stderr, output.stderr
  assert count(output.stderr, "err[check.local-inference]") == 1, output.stderr
  assert "the empty set needs an element type" in output.stderr, output.stderr
  assert count(output.stderr, "err[check.set-element-type]") == 1, output.stderr
  assert "`set.from` requires a list, not Str" in output.stderr, output.stderr

  # The rewrite is the diagnostic's fix, so `xsht lint --fix` applies it.
  let fixable = """var seen: Set[Str] = set.from(["a"])
seen = set.add(seen, "b")
seen = set.remove(seen, "a")
print \${seen.to_list().join(",")}
"""
  let candidate = test.temp_file(ctx, name: "removed.xsh", contents: bytes.from_text(fixable))?
  run.capture --text --accept=[0, 1, 2] "xsht" lint --fix $candidate
  let rewritten = candidate.read_text()?
  assert "seen = seen.add(\"b\")\n" in rewritten, rewritten
  assert "seen = seen.remove(\"a\")\n" in rewritten, rewritten
  let after = test.expect(ctx, rewritten, status: 0)?
  assert after.stdout == "b\n"
}
