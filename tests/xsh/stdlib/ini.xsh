test test_ini_decode_encode_and_files { |ctx|
  let config = ini.decode("""global: root
[server]
Host = example.test
message = hello
  world
""")?

  assert config.global == "root"
  assert config.server.host == "example.test"

  assert config.server.message == """hello
world"""

  let encoded = ini.encode({server: {message: config.server.message, host: config.server.host}, global: config.global})?
  assert "global = root" in encoded
  assert "[server]" in encoded
  assert "host = example.test" in encoded
  let config_path = test.temp_path(ctx, name: "app.ini")
  ini.write(config_path, {global: "root", server: {host: "example.test"}})
  let read_back = ini.read(config_path)?
  assert read_back.server.host == "example.test"
  test.error_kind(ini.write(config_path, {global: "again"}, overwrite: false), "ini-write")

  # Encoding fails before overwrite policy examines the existing destination.
  test.error_kind(ini.write(config_path, {global: 1}, overwrite: false), "ini-encode")
  assert ini.read(config_path)?.global == "root"
}

# The message of a rejected encode, or the empty string when the record encoded.
# `test.error_kind` compares kinds only, so message parity is asserted through
# this.
pure encode_message(result: Result[Str]) -> Str {
  match result {
    Ok(_) => ""
    Err(error) => error.message
  }
}

test test_ini_encode_orders_globals_then_sections {
  # An empty record emits no lines at all, so there is no final newline either.
  assert ini.encode({})? == ""

  # Only global keys: one `key = value` line each, in key order.
  assert ini.encode({b: "2", a: "1"})? == """a = 1
b = 2
"""

  # Only one section: its header, then its keys in key order. An empty section
  # is its header alone.
  assert ini.encode({server: {host: "example.test"}})? == """[server]
host = example.test
"""
  assert ini.encode({s: {}})? == """[s]
"""

  # Mixed: one blank line separates the global block from the first section.
  assert ini.encode({global: "root", server: {host: "example.test"}})? == """global = root

[server]
host = example.test
"""

  # Multiple sections: one blank line between consecutive sections, including
  # when a section is empty, and one final newline terminating the last line.
  assert ini.encode({s2: {b: "2"}, s1: {a: "1"}})? == """[s1]
a = 1

[s2]
b = 2
"""
  assert ini.encode({g: "1", s1: {}, s2: {}})? == """g = 1

[s1]

[s2]
"""
  assert ini.encode({a: "1", b: {c: "2"}, d: "3", e: {f: "4"}})? == """a = 1
d = 3

[b]
c = 2

[e]
f = 4
"""
}

test test_ini_encode_normalizes_section_keys {
  # Global keys keep their spelling; section keys are lowercased. The two
  # normalizations are independent, so a global key is never lowercased.
  assert ini.encode({Host: "1"})? == """Host = 1
"""
  assert ini.encode({s: {Host: "1"}})? == """[s]
host = 1
"""

  # A field replaces an earlier field with the same normalized key, and the
  # survivor is emitted once. Fields are visited in key order, not source
  # order, so the field whose spelling sorts last wins: among case variants
  # that is always the lowercase spelling, because uppercase letters sort
  # before lowercase ones.
  assert ini.encode({s: {Host: "1", host: "2"}})? == """[s]
host = 2
"""
  assert ini.encode({s: {host: "2", Host: "1"}})? == """[s]
host = 2
"""
  assert ini.encode({s: {Host: "1", host: "2", HOST: "3"}})? == """[s]
host = 2
"""
  assert ini.encode({s: {_A: "1", _a: "2"}})? == """[s]
_a = 2
"""

  # Output order is the normalized key order, which is not always the order of
  # the spellings that produced it: `A` and `a` collide even though `B` sorts
  # between them, and `Z` lowercases past `_a`.
  assert ini.encode({s: {A: "1", B: "2", a: "3"}})? == """[s]
a = 3
b = 2
"""
  assert ini.encode({s: {Z: "1", _a: "2"}})? == """[s]
_a = 2
z = 1
"""

  # Keys outside the ASCII letter range are part of the key, not separators.
  assert ini.encode({s: {"a b": "1", "k=v": "2"}})? == """[s]
a b = 1
k=v = 2
"""
}

test test_ini_encode_rejects_invalid_names {
  # A global key is validated exactly as written; an empty key and a key
  # holding NUL, newline, `[`, or `]` are rejected.
  test.error_kind(ini.encode({"a[b": "1"}), "ini-key")
  assert encode_message(ini.encode({"a[b": "1"})) == "invalid INI key"
  test.error_kind(ini.encode({"": "1"}), "ini-key")
  assert encode_message(ini.encode({"": "1"})) == "invalid INI key"
  test.error_kind(ini.encode({"a]b": "1"}), "ini-key")
  test.error_kind(
    ini.encode({
      "a\nb": "1",
    }),
    "ini-key",
  )

  # A section key is validated after normalization, so a rejected key reports
  # the lowercase spelling under the `ini-key` kind.
  test.error_kind(ini.encode({s: {"a[b": "1"}}), "ini-key")
  assert encode_message(ini.encode({s: {"a[b": "1"}})) == "invalid INI key"
  test.error_kind(ini.encode({s: {"": "1"}}), "ini-key")
  test.error_kind(ini.encode({s: {"a]b": "1"}}), "ini-key")
  test.error_kind(
    ini.encode({
      s: {
        "a\nb": "1",
      },
    }),
    "ini-key",
  )

  # Section names keep their spelling and carry their own kind.
  test.error_kind(ini.encode({"a[b": {h: "1"}}), "ini-section")
  assert encode_message(ini.encode({"a[b": {h: "1"}})) == "invalid INI section"
  test.error_kind(ini.encode({"": {h: "1"}}), "ini-section")
  test.error_kind(ini.encode({"a]b": {h: "1"}}), "ini-section")
  test.error_kind(
    ini.encode({
      "a\nb": {
        h: "1",
      },
    }),
    "ini-section",
  )

  # The whole record is collected before any global key is validated, so a
  # section-field rejection outranks a global-key rejection even when the
  # offending global key sorts first.
  test.error_kind(ini.encode({"a[b": "1", s: {c: 2}}), "ini-encode")
  assert encode_message(ini.encode({"a[b": "1", s: {c: 2}})) == "INI section values must be strings"
}

test test_ini_encode_rejects_non_string_values {
  # A top-level field must be a global string or a section record.
  test.error_kind(ini.encode({a: 1}), "ini-encode")
  assert encode_message(ini.encode({a: 1})) == "INI records may contain only global string keys or section records"
  test.error_kind(ini.encode({a: true}), "ini-encode")
  test.error_kind(ini.encode({a: null}), "ini-encode")
  test.error_kind(ini.encode({a: [1, 2]}), "ini-encode")

  # A map is not a section record.
  let empty_map: Map[Str] = {}
  test.error_kind(ini.encode({s: empty_map}), "ini-encode")
  assert encode_message(ini.encode({s: empty_map})) == "INI records may contain only global string keys or section records"

  # A section field must be a string.
  test.error_kind(ini.encode({s: {a: 1}}), "ini-encode")
  assert encode_message(ini.encode({s: {a: 1}})) == "INI section values must be strings"
  test.error_kind(ini.encode({s: {a: true}}), "ini-encode")
  test.error_kind(ini.encode({s: {a: null}}), "ini-encode")
  test.error_kind(ini.encode({s: {a: [1]}}), "ini-encode")

  # The check runs in the section's key order, so `a` is judged before `c`:
  # the rejected value is reported, and the invalid key beside it is not
  # reached. The key is a string field because `c]d` is not an identifier —
  # which is exactly why the encoder has to validate it.
  test.error_kind(ini.encode({s: {"c]d": "2", a: 1}}), "ini-encode")
  assert encode_message(ini.encode({s: {"c]d": "2", a: 1}})) == "INI section values must be strings"
}

test test_ini_encode_writes_multiline_values {
  # Embedded newlines become two-space continuation lines, so the value reads
  # back as the same string.
  assert ini.encode({
    a: """hello
world""",
  })? == """a = hello
  world
"""
  assert ini.encode({
    s: {
      message: """hello
world""",
    },
  })? == """[s]
message = hello
  world
"""
  assert ini.encode({
    a: """x

y""",
  })? == """a = x
  
  y
"""

  # A value that ends in a newline emits a continuation line holding only the
  # indent, and an empty value keeps the key line with an empty right side.
  assert ini.encode({
    a: """x
""",
  })? == """a = x
  
"""
  assert ini.encode({a: ""})? == """a = 
"""

  # The last line ends with exactly one newline, and the result re-decodes to
  # the value it was built from.
  assert ini.encode({a: "1"})? == """a = 1
"""
  assert ini.encode({a: "1"})?.ends_with("""

""") == false
  let round_trip = ini.decode(
    ini.encode({
  global: "root",
  server: {
    message: """hello
world""",
  },
})?,
  )?
  assert round_trip.global == "root"
  assert round_trip.server.message == """hello
world"""
}
