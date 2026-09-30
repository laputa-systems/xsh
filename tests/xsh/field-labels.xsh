type WireEntry = {type: Str, in: Int, match: Bool = true}
error WireError = Invalid(type: Str, in: Int)

test test_keyword_field_labels_preserve_known_types_and_wire_bytes [error] {
  let entry = WireEntry(type: "file", in: 2)
  test.eq(entry.type, "file")?
  test.eq(entry.in, 2)?
  test.ok(entry.match)?
  let {type: entry_kind, in: ordinal, match: selected, ..} = entry
  test.eq(entry_kind, "file")?
  test.eq(ordinal, 2)?
  test.ok(selected)?
  let quoted = {"type": "file", "in": 2, "match": true}
  let bare = {type: "file", in: 2, match: true}
  test.eq(json.encode(quoted)?, json.encode(bare)?)?
  test.eq(json.encode(entry)?, json.encode(bare)?)?
  let failure = WireError.Invalid(type: "bad", in: 3)
  match failure {
    WireError.Invalid {type: error_kind, in: error_number} => {
      test.eq(error_kind, "bad")?
      test.eq(error_number, 3)?
    },
    _ => { test.fail("keyword error payload labels must match")? },
  }
  match bare {
    {type: kind, in: number, match: enabled} => {
      test.eq(kind, "file")?
      test.eq(number, 2)?
      test.ok(enabled)?
    },
    _ => { test.fail("keyword field labels must match")? },
  }
  var mutable = bare
  mutable.type = "directory"
  test.eq(mutable.type, "directory")?
  test.eq(bare.type, "file")?
}

test test_keyword_field_labels_across_keyword_spellings [error] { |ctx|
  for label in ["and", "assert", "break", "const", "continue", "defer", "else", "enum", "export", "false", "for", "guard", "if", "in", "let", "loop", "match", "not", "null", "or", "proc", "pure", "retry", "return", "run", "spawn", "stream", "test", "true", "try", "type", "unless", "use", "var", "wait", "when", "with", "yield"] {
    let source = "type Wire = {" + label + ": Int}\nlet row = Wire(" + label + ": 1)\nlet {" + label + ": selected, ..} = row\nprint $selected\nprint $row." + label + "\n"
    let executed = test.run_script(ctx, source)?
    test.ok(executed.success, executed.stderr)?
    test.eq(executed.stdout, "1\n1\n")?
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
    test.ok(! rejected.success, source + rejected.stderr)?
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
      test.ok(! rejected.success, source + rejected.stderr)?
    }
    let argv = test.run_script(ctx, "run printf \"%s\\n\" " + label + "\n")?
    test.ok(argv.success, argv.stderr)?
    test.eq(argv.stdout, label + "\n")?
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
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "file\n2\n")?
  for invalid in [
    r"""let raw = json.decode("{\"type\":\"wrong\"}")?
let selected: Int = raw.type
""",
    "type Entry = {type: Int, type: Str}\n",
    "type Entry = {type: Int}\nlet row = Entry(type: 1, type: 2)\n",
    "pure selected(value: Int) -> Int { value }\nlet result = selected(type: 1)\n",
  ] {
    let rejected = test.run_script(ctx, invalid)?
    test.ok(! rejected.success, invalid + rejected.stderr)?
  }
}

test test_keyword_field_label_tooling_preserves_execution_and_converges [fs, process, error] { |ctx|
  let source = r"""let row = {"type": "file", r"in": 2, "wire.type": 3} # Keep the wire explanation.
let label: Str = row.get("type")?
print $label $row.in ${row["wire.type"]}
"""
  let before = test.run_script(ctx, source)?
  test.ok(before.success, before.stderr)?
  let candidate = test.temp_file(ctx, name: "field-label-fix.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(applied.status.exited_with(0), applied.stderr)?
  let fixed = candidate.read_text()?
  test.ok("{type: \"file\", in: 2, \"wire.type\": 3}" in fixed)?
  test.ok("label = row.get(\"type\")?" in fixed)?
  test.ok("# Keep the wire explanation." in fixed)?
  let after = test.run_script(ctx, fixed)?
  test.ok(after.success, after.stderr)?
  test.eq(after.stdout, before.stdout)?
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(repeated.status.exited_with(0), repeated.stderr)?
  test.eq(candidate.read_text()?, fixed)?
}
