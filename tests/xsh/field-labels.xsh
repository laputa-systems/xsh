type WireEntry = {type: Str, in: Int, match: Bool = true}
error WireError = Invalid(type: Str, in: Int)

proc test_keyword_field_labels_preserve_known_types_and_wire_bytes() [error] {
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
