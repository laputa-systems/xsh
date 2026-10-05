# `Union[A, B, ...]` is a closed set of member types. A value fits it only by
# fitting a member, is narrowed by `is` and type patterns, and has no runtime
# wrapper: the value is whichever member it is.

type Word = Union[Str, Path]

type Named = {name: Str, size: Int}

type Item = Union[Named, List[Str], Int, Str, Path]

enum Mode: Str { Fast = "fast", Slow = "slow" }

type Task = {tool: Str, args: List[Word], level: Union[Int, Mode]}

type TextFirst = {mode: Union[Str, Mode]}

type ModeFirst = {mode: Union[Mode, Str]}

pure describe(word: Word) -> Str {
  match word {
    text is Str => f"text {text}"
    file is Path => f"file {file.name()}"
  }
}

# Each failed test removes its member, so the tail sees the one that is left.
pure show(item: Item) -> Str {
  return f"named {item.name} {item.size + 1}" when item is Named

  if item is List[Str] {
    var joined = ""
    for part in item {
      joined = joined + part
    }

    return f"list {item.len()} {item[0]} {joined}"
  }

  return f"int {item * 2} {item < 3}" when item is Int
  return "str " + item when item is Str

  f"path {item.name()}"
}

pure number_or_text(value: Union[Int, Float, Str]) -> Str {
  if value is (Int | Float) {
    let number: Union[Int, Float] = value
    return if number is Int { "int" } else { "float" }
  }

  value
}

type Holder = {word: Word, level: Union[Int, Str]}

pure file_name_or_marked(word: Word) -> Str {
  guard word is Path else {
    return word + "!"
  }

  word.name()
}

pure pick(maybe: Word?, fallback: Word = "default") -> Str {
  let word = maybe ?? fallback
  return maybe.upper() when maybe != null and maybe is Str

  if let file is Path = word {
    return file.name()
  }

  word
}

pure path_name_bytes(words: List[Word]) -> Int {
  var total = 0
  for word in words {
    continue unless word is Path
    total += word.name().byte_len()
  }

  total
}

# The motivating case: an argv word list holds text and paths, and both reach
# the child as their own bytes with no `.display()`.
type Payload = Union[Str, Path, List[Str], Bytes]

# Each call is on the same slot, declared as the union; the member the test
# left decides which methods it has.
pure measure(payload: Payload) -> Str {
  return "text " + payload.trim() when payload is Str
  return "file " + payload.name() when payload is Path

  return "list " + payload.join("+") when payload is List[Str]

  f"bytes {payload.len()}"
}

pure measure_present(payload: Payload?) -> Str {
  guard payload != null else {
    return "absent"
  }

  return f"bytes {payload.len()}" when payload is Bytes

  if payload is (Str | Path) {
    return if payload is Path { payload.name() } else { payload.trim() }
  }

  f"list {payload.len()}"
}

test test_method_on_a_narrowed_union_slot_is_the_narrowed_member_s {
  assert measure("  padded ") == "text padded"
  assert measure(/srv/data.txt) == "file data.txt"
  assert measure(["a", "b"]) == "list a+b"
  assert measure(bytes.from_text("abc")) == "bytes 3"

  assert measure_present(null) == "absent"
  assert measure_present(bytes.from_text("abcd")) == "bytes 4"
  assert measure_present(/srv/data.txt) == "data.txt"
  assert measure_present(" padded ") == "padded"
  assert measure_present(["a", "b", "c"]) == "list 3"
}

test test_union_word_list_splices_into_argv { |ctx|
  let dir = test.temp_dir(ctx, name: "union-argv")?
  let file = fp"{dir}/input file.txt"
  file.write("payload\n")
  let words: List[Word] = ["--", file, "literal"]
  let listed = run.capture --text printf "%s\n" @words
  assert listed.stdout == f"--\n{file}\nliteral\n"

  let files = [word for word in words if word is Path]
  let read = run.capture --text cat @files
  assert read.stdout == "payload\n"

  let first: Word = file
  let one = run.capture --text printf "%s" $first
  assert one.stdout == f"{file}"
}

test test_union_narrows_on_both_sides_of_a_type_test {
  let items: List[Item] = [{name: "n", size: 2}, ["a", "b"], 2, "s", /tmp/p]
  assert [show(item) for item in items] == [
    "named n 3",
    "list 2 a ab",
    "int 4 true",
    "str s",
    "path p",
  ]
  assert number_or_text(1) == "int"
  assert number_or_text(1.5) == "float"
  assert number_or_text("text") == "text"
}

# Every form that carries a narrowing fact carries it for a union: `guard`,
# a guarded statement, `if let`, and a null test before the type test.
test test_union_narrows_through_guards_pattern_conditions_and_optionals {
  assert file_name_or_marked("a") == "a!"
  assert file_name_or_marked(/tmp/b) == "b"
  assert pick(null) == "default"
  assert pick("s") == "S"
  assert pick(/tmp/q) == "q"
  assert pick(null, /tmp/z) == "z"
  assert path_name_bytes(["a", /tmp/abc, /tmp/de]) == 5

  var holder = Holder(word: "a", level: 1)
  holder.level = "high"
  holder.word = /tmp/x
  assert holder.level == "high"
  assert holder.word == /tmp/x
}

test test_union_match_covers_every_member {
  assert describe("all") == "text all"
  assert describe(/src/main.c) == "file main.c"

  var current: Item = 1
  current = "later"
  let seen = match current {
    named is Named => named.name,
    parts is List[Str] => parts[0],
    number is Int => f"{number}",
    text is Str => text.upper(),
    file is Path => file.name(),
  }
  assert seen == "LATER"
}

test test_union_fits_a_wider_union_and_compares_by_value {
  let word: Word = "x"
  let wide: Union[Str, Path, Int] = word
  let maybe: Word? = word
  let absent: Word? = null
  assert wide == "x"
  assert maybe == "x"
  assert absent == null
  assert word != p"x"

  let words: List[Word] = ["x", p"y"]
  assert word in words
  assert p"y" in words
  assert "z" not in words

  let texts = ["a", "b"]
  let widened: List[Word] = [text for text in texts]
  assert widened.len() == 2 and widened[1] == "b"

  let chosen: Word = if texts.len() == 2 { "a" } else { p"b" }
  assert chosen == "a"

  let record = {word: word, values: {a: 1, b: "two"}}
  assert record.word == "x"
  let values: Map[Union[Int, Str]] = {a: 1, b: "two"}
  assert values["b"] == "two"
}

test test_require_and_type_tests_validate_a_dynamic_value_as_a_union {
  let decoded = json.decode("[\"make\", \"all\"]")?.require(List[Word])?
  assert describe(decoded[1]) == "text all"

  let mixed = json.decode("[\"make\", 1]")?.require(List[Word])
  assert mixed is Err(_)

  let file: Any = /x
  let number: Any = 1
  let narrowed = if let word is Word = file { describe(word) } else { "other" }
  assert narrowed == "file x"
  assert ! (number is Word)
  assert number.require(Word) is Err(_)

  # A passing test leaves the dynamic value typed as the union, which a test
  # for some of its members then splits.
  assert file is Word
  assert file is (Str | Path)
  assert describe(file) == "file x"
}

# A value belongs to the first member, in the order written, that accepts it.
# The order shows only where a member converts: a Str-backed enum after `Str`
# never converts a string.
test test_union_schema_slots_try_members_in_the_order_written {
  let task = json.decode("{\"tool\": \"make\", \"args\": [\"all\"], \"level\": \"fast\"}")?.require(Task)?
  assert task.level is Mode
  assert task.level == Fast
  assert describe(task.args[0]) == "text all"

  let numbered = json.decode("{\"tool\": \"make\", \"args\": [], \"level\": 3}")?.require(Task)?
  assert numbered.level == 3

  let unknown = json.decode("{\"tool\": \"make\", \"args\": [], \"level\": \"warp\"}")?.require(Task)
  assert unknown is Err(_)

  let fast = json.decode("{\"mode\": \"fast\"}")?
  let text_first = fast.require(TextFirst)?
  assert text_first.mode is Str
  let mode_first = fast.require(ModeFirst)?
  assert mode_first.mode is Mode
  let other = json.decode("{\"mode\": \"other\"}")?.require(ModeFirst)?
  assert other.mode is Str
  assert json.encode(mode_first)? == "{\"mode\":\"fast\"}"
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

# A union is never simplified, so each shape a simplifier would rewrite is an
# error instead.
test test_union_type_rejects_every_form_it_would_have_to_simplify { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Word = Union[Str, Path]
type One = Union[Str]
type Twice = Union[Str, Str]
type Contained = Union[Int, UInt]
type Loose = Union[Str, Any]
type Maybe = Union[Str, Int?]
type Nullable = Union[Str, Null]
type Nested = Union[Word, Int]
type Lazy = Union[Str, Stream[Int]]
type Either[T] = Union[T, Str]
type Collapsed = Either[Str]
type Kept = Either[Int]

error BuildError {
  Failed(reason: Str)
}

type Family = Union[BuildError, Error]

let kept: Kept = 1
print ${kept == 1}
""",
  )?
  assert count(stderr, "err[check.union-type]") == 10, stderr
  assert "a union lists at least two member types" in stderr, stderr
  assert "`Str` is listed twice" in stderr, stderr
  assert "every `Int` already fits the member `UInt`" in stderr, stderr
  assert "`Any` already accepts every value" in stderr, stderr
  assert "write `Union[...]?` around the non-null members" in stderr, stderr
  assert "a member cannot be another union; list its members here" in stderr, stderr
  assert "a member cannot be a stream" in stderr, stderr
  assert "every `BuildError` already fits the member `Error`" in stderr, stderr
}

test test_unnarrowed_union_rejects_operations_that_read_one_member { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Word = Union[Str, Path]
type Named = {name: Str}
type Shape = Union[Named, List[Str]]
type Number = Union[Int, Float]

proc consume(word: Word, shape: Shape, number: Number) [process, error] {
  let a = word.name()
  let b = shape.name
  let c = shape[0]
  for part in shape {
    print $part
  }

  let d = number < 2
  let e = f"{word}"
  run echo $number ?
  print $e
}

consume("a", ["b"], 1)?
""",
  )?
  assert count(stderr, "err[check.union-narrow]") == 7, stderr
  assert "Union[Str, Path] must be narrowed to one member before calling `name`" in stderr, stderr
  assert "before reading `.name`" in stderr, stderr
  assert "before indexing" in stderr, stderr
  assert "before iteration" in stderr, stderr
  assert "before an ordering comparison" in stderr, stderr
  assert "match it with one `name is Member` arm per member" in stderr, stderr
}

# A union does not become a member, a wider union does not become a narrower
# one, and a list of a member is not a list of the union.
test test_union_assignability_follows_the_existing_rules { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Word = Union[Str, Path]
type Wide = Union[Str, Path, Int]

pure consume(word: Word, wide: Wide, texts: List[Str], number: Union[Int, Float]) -> Int {
  let a: Str = word
  let b: Word = wide
  let c: List[Word] = texts
  let d: Word = 1
  let e = number + 1
  let f = word == 1
  let g = if texts.len() == 0 { "a" } else { p"b" }
  let h: Any = word
  let i: Wide = word
  0
}

print ${consume("a", 1, [], 1)}
""",
  )?
  assert "expected Str, found Union[Str, Path]" in stderr, stderr
  assert "a union value is a Str only after `value is Str`" in stderr, stderr
  assert "expected Union[Str, Path], found Union[Str, Path, Int]" in stderr, stderr
  assert "expected List[Union[Str, Path]], found List[Str]" in stderr, stderr
  assert "a List is invariant in its element type" in stderr, stderr
  assert "expected Union[Str, Path], found Int" in stderr, stderr
  assert "expected Int, found Union[Int, Float]" in stderr, stderr
  # The branches that disagree are reported once, at the second branch.
  assert count(stderr, "expected Str, found Path") == 1, stderr
  assert count(stderr, "err[check.type-mismatch]") == 7, stderr
  assert count(stderr, "err[") == 7, stderr
}

test test_union_type_tests_name_members_and_matches_cover_them { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Wide = Union[Str, Path, Int]

proc consume(wide: Wide) [error] -> Result[Str] {
  if wide is Float {
    return "never"
  }

  match wide {
    text is Str => print $text
    _ is Int => print "int"
  }

  let guarded = match wide {
    text is Str => text
    file is Path if file.name() == "x" => "x"
    _ is Int => "int"
  }
  let covered = match wide {
    text is Str => text
    _ is Path | _ is Int => "other"
  }
  guarded + covered
}

print ${consume("a")?}
""",
  )?
  assert "`Float` is not a member of Union[Str, Path, Int]" in stderr, stderr
  assert count(stderr, "err[check.pattern-type]") == 1, stderr
  assert "non-exhaustive match: missing member(s) `Path`" in stderr, stderr
  assert count(stderr, "err[check.non-exhaustive-match]") == 1, stderr
  assert "warn[" not in stderr, stderr
  assert "value-producing match must be exhaustive: missing member(s) `Path`" in stderr, stderr
  assert count(stderr, "err[check.match-value-exhaustive]") == 1, stderr
}

# The migration lint names the union of a `List[Any]` that is only ever
# filled from typed list literals. It has no fix: the union changes the
# binding's type, so `--fix` leaves the file alone, and the suggested type
# then checks and runs.
test test_lint_names_the_union_of_a_list_any_filled_from_typed_literals { |ctx|
  let source = "proc compile(cc: Path, flags: List[Str]) [process, error] {\n  var argv: List[Any] = [\"-c\", cc]\n  argv = [@argv, @flags]\n  argv += [\"-o\", /dev/null]\n  print f\"{argv.len()}\"\n}\n\ncompile(/usr/bin/cc, [\"-O2\"])?\n"
  let file = test.temp_file(ctx, name: "argv.xsh", contents: bytes.from_text(source))?
  let reported = run.capture --text "xsht" lint --only lint.list-any-union $file
  assert count(reported.stderr, "warn[lint.list-any-union]") == 1, reported.stderr
  assert "the closed type is `List[Union[Str, Path]]`" in reported.stderr, reported.stderr

  let fixed = run.capture --text "xsht" lint --fix --only lint.list-any-union $file
  assert file.read_text()? == source, fixed.stderr

  let migrated = source.replace("List[Any]", with: "List[Union[Str, Path]]")
  let output = test.expect(ctx, migrated, status: 0)?
  assert output.stdout == "5\n"
}
