type WireEntry = {type: Str, in: Int, match: Bool = true}

error WireError = Invalid(type: Str, in: Int)

test test_keyword_field_labels_preserve_known_types_and_wire_bytes {
  let entry = WireEntry("file", 2)
  assert entry.type == "file"
  assert entry.in == 2
  assert entry.match
  let {type: entry_kind, in: ordinal, match: selected, ..} = entry
  assert entry_kind == "file"
  assert ordinal == 2
  assert selected, "destructured keyword label keeps its value"
  let quoted = json.decode(r"""{"type":"file","in":2,"match":true}""")?
  let bare = {type: "file", in: 2, match: true}
  assert json.encode(quoted)? == json.encode(bare)?
  assert json.encode(entry)? == json.encode(bare)?
  let failure = WireError.Invalid(type: "bad", in: 3)
  if let .Invalid {type: error_kind, in: error_number} = failure {
    assert error_kind == "bad"
    assert error_number == 3
  } else {
    test.fail("keyword error payload labels must match")
  }

  if let {type: kind, in: number, match: enabled} = bare {
    assert kind == "file"
    assert number == 2
    assert enabled, "pattern-bound keyword field remains enabled"
  } else {
    test.fail("keyword field labels must match")
  }

  var mutable = bare
  mutable.type = "directory"
  assert mutable.type == "directory"
  assert bare.type == "file"
}

type WireMeta = {type: Str, in: Int}

type WireRow = {name: Str, meta: WireMeta}

test test_keyword_field_labels_in_update_paths_and_constant_map_keys { |ctx|
  let row = WireRow("entry", WireMeta("file", 1))
  let updated = {...row, meta.type: "directory", meta.in: 2}
  assert updated.meta.type == "directory"
  assert updated.meta.in == 2
  assert row.meta.type == "file"
  let bare: Map[Int] = {type: 1, match: 2}
  assert bare.get("type")? == 1
  assert bare.keys() == ["match", "type"]
  let duplicate = test.run_script(
    ctx,
    """let row = {type: 1, "type": 2}
""",
  )?
  assert ! duplicate.success, duplicate.stderr
  assert "check.duplicate-record-field" in duplicate.stderr
}

test test_keyword_field_labels_across_keyword_spellings { |ctx|
  for label in [
    "and",
    "assert",
    "break",
    "const",
    "continue",
    "defer",
    "else",
    "enum",
    "export",
    "false",
    "for",
    "guard",
    "if",
    "in",
    "let",
    "loop",
    "match",
    "not",
    "null",
    "or",
    "proc",
    "pure",
    "retry",
    "return",
    "run",
    "spawn",
    "stream",
    "test",
    "true",
    "try",
    "type",
    "unless",
    "use",
    "var",
    "wait",
    "when",
    "with",
    "yield",
  ] {
    let source = "type Wire = {" + label + """: Int}
let row = Wire(""" + label + """: 1)
let {""" + label + """: selected, ..} = row
print $selected
print $row.""" + label + "\n"
    let executed = test.expect(ctx, source, status: 0)?
    assert executed.stdout == """1
1
"""
  }
}

test test_keyword_field_labels_reject_keyword_bindings_puns_and_module_shadowing { |ctx|
  for source in [
    """let type = 1
""",
    """pure value(in: Int) -> Int { 1 }
""",
    """type match = {value: Int}
""",
    """use fs as type
""",
    """let row = {type}
""",
    """let row = {type: 1}
let {type} = row
""",
    """type Entry = {type: Int}
let row = Entry(type:)
""",
    """let row = {type: 1}
match row { {type} => { print 1 }, _ => { print 2 } }
""",
    """let {json: json} = {json: 1}
""",
    """let row = {json: 1}
match row { {json} => { print 1 }, _ => { print 2 } }
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert ! rejected.success, source + rejected.stderr
  }
}

test test_keyword_field_labels_reject_introducer_bindings_and_puns { |ctx|
  for label in ["assert", "const", "enum", "test", "try"] {
    for source in [
      "let " + label + """ = 1
""",
      "let row = {" + label + """}
""",
      "let row = {" + label + """: 1}
let {""" + label + """} = row
""",
      "type Entry = {" + label + """: Int}
let row = Entry(""" + label + """:)
""",
      "let row = {" + label + """: 1}
match row { {""" + label + """} => {}, _ => {} }
""",
    ] {
      let rejected = test.run_script(ctx, source)?
      assert ! rejected.success, source + rejected.stderr
    }

    let argv = test.expect(ctx, "run printf \"%s\\n\" " + label + "\n", status: 0)?
    assert argv.stdout == label + "\n"
  }
}

test test_keyword_field_labels_retain_dynamic_validation_and_duplicate_checks { |ctx|
  let source = r"""type Entry = {type: Str, in: Int}
let raw = json.decode("{\"type\":\"file\",\"in\":2}")?
let row = raw.require(Entry)?
print $row.type
print $row.in
"""
  let executed = test.expect(ctx, source, status: 0)?
  assert executed.stdout == """file
2
"""
  for invalid in [
    r"""let raw = json.decode("{\"type\":\"wrong\"}")?
let selected: Int = raw.type
""",
    """type Entry = {type: Int, type: Str}
""",
    """type Entry = {type: Int}
let row = Entry(type: 1, type: 2)
""",
    """pure selected(value: Int) -> Int { value }
let result = selected(type: 1)
""",
  ] {
    let rejected = test.run_script(ctx, invalid)?
    assert ! rejected.success, invalid + rejected.stderr
  }
}

test test_keyword_field_label_tooling_preserves_execution_and_converges { |ctx|
  let source = r"""let row = {"type": "file", r"in": 2, "wire.type": 3} # Keep the wire explanation.
let label: Str = row.get("type")?
print $label $row.in ${row["wire.type"]}
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "field-label-fix.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $candidate
  assert applied.status.exited_with(0), applied.stderr
  let fixed = candidate.read_text()?
  assert "{type: \"file\", in: 2, \"wire.type\": 3}" in fixed
  assert "label = row.type" in fixed
  assert "# Keep the wire explanation." in fixed
  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == before.stdout
  let repeated = run.capture --text "xsht" lint --fix $candidate
  assert repeated.status.exited_with(0), repeated.stderr
  assert candidate.read_text()? == fixed
}

# The entries of one brace or parenthesis list, with the entry under test
# first, in the middle, or last among the other two.
pure placed(position: Int, entry: Str, before: Str, after: Str) -> Str {
  let entries = if position == 0 {
    [entry, before, after]
  } else if position == 1 {
    [before, entry, after]
  } else {
    [before, after, entry]
  }
  entries.join(", ")
}

# Every keyword spelling, including the ones that also begin an expression
# (`null`, `true`, `false`, `if`, `match`, `not`, `try`, `run`), labels a
# field wherever a `:` follows it directly.
test test_every_keyword_labels_a_field_at_each_position_of_each_record_form { |ctx|
  for label in [
    "and",
    "assert",
    "break",
    "const",
    "continue",
    "defer",
    "else",
    "enum",
    "errdefer",
    "export",
    "false",
    "for",
    "guard",
    "if",
    "in",
    "let",
    "loop",
    "match",
    "not",
    "null",
    "or",
    "proc",
    "pure",
    "retry",
    "return",
    "run",
    "spawn",
    "stream",
    "true",
    "try",
    "type",
    "unless",
    "use",
    "var",
    "wait",
    "when",
    "while",
    "with",
    "yield",
  ] {
    var source = ""
    for position in [0, 1, 2] {
      let n = f"{position}"
      let schema = placed(position, f"{label}: Int", "a: Int", "z: Int")
      let fields = placed(position, f"{label}: 2", "a: 1", "z: 3")
      let update = placed(position, f"{label}: 20", "a: 10", "z: 30")
      let bound = placed(position, f"{label}: got{n}", f"a: low{n}", f"z: high{n}")
      let matched = placed(position, f"{label}: picked", "a: left", "z: right")
      source += "type Wire" + n + " = {" + schema + "}\n"
      source += "let row" + n + " = {" + fields + "}\n"
      source += "let built" + n + " = Wire" + n + "(" + fields + ")\n"
      source += "let updated" + n + " = {...built" + n + ", " + update + "}\n"
      source += "let {" + bound + "} = row" + n + "\n"
      source += "if let {" + matched + "} = updated" + n + " {\n"
      source += "  print $picked $left $right\n} else {\n  print unmatched\n}\n"
      source += "print $built" + n + "." + label + " $got" + n
      source += " $low" + n + " $high" + n + "\n"
    }

    let executed = test.expect(ctx, source, status: 0)?
    let placement = "20 10 30\n2 2 1 3\n"
    assert executed.stdout == placement + placement + placement, label + executed.stdout
  }
}

test test_literal_word_labels_are_fields_beside_the_sets_of_those_words { |ctx|
  let options = {
    gnu: {status: 2},
    null: {form: "-0 --null", default: false},
    help: {form: "--help", default: false, stop: true},
  }
  assert options.null.form == "-0 --null"
  assert options.help.stop, "the field after a literal-word label is still read"
  let leading = {null: 1, true: 2, false: 3}
  assert leading.null + leading.true + leading.false == 6
  let trailing = {a: null, b: true, false: false, true: true, null: null}
  assert trailing.true and ! trailing.false, "a literal word labels a field and is its value"
  assert trailing.null == null
  let counts: Map[Int] = {true: 1, a: 2, null: 3}
  assert counts.keys() == ["a", "null", "true"]
  let nested = {...{row: leading}, row.null: 7, row.true: 8}
  assert nested.row.null == 7
  assert nested.row.true == 8
  assert nested.row.false == 3

  assert {true, false}.to_list() == [false, true]
  assert {false, true, false}.len() == 2
  let flags: Set[Bool] = {true,}
  assert true in flags
  assert {"a", "b"}.to_list() == ["a", "b"]
  let chosen = {if leading.null == 1 { "one" } else { "other" }, "two"}
  assert chosen.to_list() == ["one", "two"]

  for source in [
    "let mixed = {1, null: 2}\n",
    "let mixed = {true, null: 2}\n",
    "let mixed = {null: 1, true}\n",
    "let mixed = {a: 1, false, b: 2}\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert ! rejected.success, source
    assert "parse.brace-literal-mixed" in rejected.stderr, source + rejected.stderr
  }
}
