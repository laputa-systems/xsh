type WireEntry = {type: Str, in: Int, match: Bool = true}
error WireError = Invalid(type: Str, in: Int)

test test_keyword_field_labels_preserve_known_types_and_wire_bytes [error] {
  let entry = WireEntry(type: "file", in: 2)
  entry.type == "file"
  entry.in == 2
  entry.match
  let {type: entry_kind, in: ordinal, match: selected, ..} = entry
  entry_kind == "file"
  ordinal == 2
  assert selected, "destructured keyword label keeps its value"
  let quoted = json.decode(r"""{"type":"file","in":2,"match":true}""")?
  let bare = {type: "file", in: 2, match: true}
  json.encode(quoted)? == json.encode(bare)?
  json.encode(entry)? == json.encode(bare)?
  let failure = WireError.Invalid(type: "bad", in: 3)
  if let WireError.Invalid {type: error_kind, in: error_number} = failure {
    error_kind == "bad"
    error_number == 3
  } else {
    test.fail("keyword error payload labels must match")?
  }
  if let {type: kind, in: number, match: enabled} = bare {
    kind == "file"
    number == 2
    assert enabled, "pattern-bound keyword field remains enabled"
  } else {
    test.fail("keyword field labels must match")?
  }
  var mutable = bare
  mutable.type = "directory"
  mutable.type == "directory"
  bare.type == "file"
}

test test_keyword_field_labels_across_keyword_spellings [error] { |ctx|
  for label in ["and", "assert", "break", "const", "continue", "defer", "else", "enum", "export", "false", "for", "guard", "if", "in", "let", "loop", "match", "not", "null", "or", "proc", "pure", "retry", "return", "run", "spawn", "stream", "test", "true", "try", "type", "unless", "use", "var", "wait", "when", "with", "yield"] {
    let source = "type Wire = {" + label + ": Int}\nlet row = Wire(" + label + ": 1)\nlet {" + label + ": selected, ..} = row\nprint $selected\nprint $row." + label + "\n"
    let executed = test.run_script(ctx, source)?
    assert executed.success, executed.stderr
    executed.stdout == "1\n1\n"
  }
}

test test_keyword_field_labels_reject_keyword_bindings_puns_and_module_shadowing [error] { |ctx|
  for source in [
    "let type = 1\n",
    "pure value(in: Int) -> Int { 1 }\n",
    "type match = {value: Int}\n",
    "use fs as type\n",
    "let row = {type}\n",
    "let row = {type: 1}\nlet {type} = row\n",
    "type Entry = {type: Int}\nlet row = Entry(type:)\n",
    "let row = {type: 1}\nmatch row { {type} => { print 1 }, _ => { print 2 } }\n",
    "let {json: json} = {json: 1}\n",
    "let row = {json: 1}\nmatch row { {json} => { print 1 }, _ => { print 2 } }\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert ! rejected.success, source + rejected.stderr
  }
}

test test_keyword_field_labels_reject_introducer_bindings_and_puns [error] { |ctx|
  for label in ["assert", "const", "enum", "test", "try"] {
    for source in [
      "let " + label + " = 1\n",
      "let row = {" + label + "}\n",
      "let row = {" + label + ": 1}\nlet {" + label + "} = row\n",
      "type Entry = {" + label + ": Int}\nlet row = Entry(" + label + ":)\n",
      "let row = {" + label + ": 1}\nmatch row { {" + label + "} => {}, _ => {} }\n",
    ] {
      let rejected = test.run_script(ctx, source)?
      assert ! rejected.success, source + rejected.stderr
    }
    let argv = test.run_script(ctx, "run printf \"%s\\n\" " + label + "\n")?
    assert argv.success, argv.stderr
    argv.stdout == label + "\n"
  }
}

test test_keyword_field_labels_retain_dynamic_validation_and_duplicate_checks [error] { |ctx|
  let source = r"""type Entry = {type: Str, in: Int}
let raw = json.decode("{\"type\":\"file\",\"in\":2}")?
let row = raw.require(Entry)?
print $row.type
print $row.in
"""
  let executed = test.run_script(ctx, source)?
  assert executed.success, executed.stderr
  executed.stdout == "file\n2\n"
  for invalid in [
    r"""let raw = json.decode("{\"type\":\"wrong\"}")?
let selected: Int = raw.type
""",
    "type Entry = {type: Int, type: Str}\n",
    "type Entry = {type: Int}\nlet row = Entry(type: 1, type: 2)\n",
    "pure selected(value: Int) -> Int { value }\nlet result = selected(type: 1)\n",
  ] {
    let rejected = test.run_script(ctx, invalid)?
    assert ! rejected.success, invalid + rejected.stderr
  }
}

test test_keyword_field_label_tooling_preserves_execution_and_converges [fs, process, error] { |ctx|
  let source = r"""let row = {"type": "file", r"in": 2, "wire.type": 3} # Keep the wire explanation.
let label: Str = row.get("type")?
print $label $row.in ${row["wire.type"]}
"""
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr
  let candidate = test.temp_file(ctx, name: "field-label-fix.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  assert applied.status.exited_with(0), applied.stderr
  let fixed = candidate.read_text()?
  "{type: \"file\", in: 2, \"wire.type\": 3}" in fixed
  "label = row.type" in fixed
  "# Keep the wire explanation." in fixed
  let after = test.run_script(ctx, fixed)?
  assert after.success, after.stderr
  after.stdout == before.stdout
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  assert repeated.status.exited_with(0), repeated.stderr
  candidate.read_text()? == fixed
}
