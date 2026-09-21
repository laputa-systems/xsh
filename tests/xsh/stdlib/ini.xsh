proc test_ini_decode_encode_and_files(ctx: TestContext) [fs, error] {
  let config = ini.decode("""global: root
[server]
Host = example.test
message = hello
  world
""")?

  test.eq(config.global, "root")?
  test.eq(config.server.host, "example.test")?

  test.eq(
    config.server.message,
    """hello
world""",
  )?

  let encoded = ini.encode({server: {message: config.server.message, host: config.server.host}, global: config.global})?
  test.contains(encoded, "global = root")?
  test.contains(encoded, "[server]")?
  test.contains(encoded, "host = example.test")?
  let config_path = test.temp_path(ctx, name: "app.ini")
  ini.write(config_path, {global: "root", server: {host: "example.test"}})?
  let read_back = ini.read(config_path)?
  test.eq(read_back.server.host, "example.test")?
  test.error_kind(ini.write(config_path, {global: "again"}, overwrite: false), "ini-write")?
}

# The message of a rejected encode, or the empty string when the record encoded.
# `test.error_kind` compares kinds only, so message parity is asserted through
# this.
pure encode_message(result: Result[Str]) -> Str {
  match result {
    Ok(_) => return ""
    Err(error) => return error.message
  }
}

proc test_ini_encode_orders_globals_then_sections() [fs, error] {
  # An empty record emits no lines at all, so there is no final newline either.
  test.eq(ini.encode({})?, "")?

  # Only global keys: one `key = value` line each, in key order.
  test.eq(
    ini.encode({b: "2", a: "1"})?,
    """a = 1
b = 2
""",
  )?

  # Only one section: its header, then its keys in key order. An empty section
  # is its header alone.
  test.eq(
    ini.encode({server: {host: "example.test"}})?,
    """[server]
host = example.test
""",
  )?
  test.eq(
    ini.encode({s: {}})?,
    """[s]
""",
  )?

  # Mixed: one blank line separates the global block from the first section.
  test.eq(
    ini.encode({global: "root", server: {host: "example.test"}})?,
    """global = root

[server]
host = example.test
""",
  )?

  # Multiple sections: one blank line between consecutive sections, including
  # when a section is empty, and one final newline terminating the last line.
  test.eq(
    ini.encode({s2: {b: "2"}, s1: {a: "1"}})?,
    """[s1]
a = 1

[s2]
b = 2
""",
  )?
  test.eq(
    ini.encode({g: "1", s1: {}, s2: {}})?,
    """g = 1

[s1]

[s2]
""",
  )?
  test.eq(
    ini.encode({a: "1", b: {c: "2"}, d: "3", e: {f: "4"}})?,
    """a = 1
d = 3

[b]
c = 2

[e]
f = 4
""",
  )?
}

proc test_ini_encode_normalizes_section_keys() [fs, error] {
  # Global keys keep their spelling; section keys are lowercased. The two
  # normalizations are independent, so a global key is never lowercased.
  test.eq(
    ini.encode({Host: "1"})?,
    """Host = 1
""",
  )?
  test.eq(
    ini.encode({s: {Host: "1"}})?,
    """[s]
host = 1
""",
  )?

  # A field replaces an earlier field with the same normalized key, and the
  # survivor is emitted once. Fields are visited in key order, not source
  # order, so the field whose spelling sorts last wins: among case variants
  # that is always the lowercase spelling, because uppercase letters sort
  # before lowercase ones.
  test.eq(
    ini.encode({s: {Host: "1", host: "2"}})?,
    """[s]
host = 2
""",
  )?
  test.eq(
    ini.encode({s: {host: "2", Host: "1"}})?,
    """[s]
host = 2
""",
  )?
  test.eq(
    ini.encode({s: {Host: "1", host: "2", HOST: "3"}})?,
    """[s]
host = 2
""",
  )?
  test.eq(
    ini.encode({s: {_A: "1", _a: "2"}})?,
    """[s]
_a = 2
""",
  )?

  # Output order is the normalized key order, which is not always the order of
  # the spellings that produced it: `A` and `a` collide even though `B` sorts
  # between them, and `Z` lowercases past `_a`.
  test.eq(
    ini.encode({s: {A: "1", B: "2", a: "3"}})?,
    """[s]
a = 3
b = 2
""",
  )?
  test.eq(
    ini.encode({s: {Z: "1", _a: "2"}})?,
    """[s]
_a = 2
z = 1
""",
  )?

  # Keys outside the ASCII letter range are part of the key, not separators.
  test.eq(
    ini.encode({s: {a b: "1", k=v: "2"}})?,
    """[s]
a b = 1
k=v = 2
""",
  )?
}

proc test_ini_encode_rejects_invalid_names() [fs, error] {
  # A global key is validated exactly as written; an empty key and a key
  # holding NUL, newline, `[`, or `]` are rejected.
  test.error_kind(ini.encode({a[b: "1"}), "ini-key")?
  test.eq(encode_message(ini.encode({a[b: "1"})), "invalid INI key")?
  test.error_kind(ini.encode({: "1"}), "ini-key")?
  test.eq(encode_message(ini.encode({: "1"})), "invalid INI key")?
  test.error_kind(ini.encode({a]b: "1"}), "ini-key")?
  test.error_kind(
    ini.encode(
      {
        a
      b: "1",
      },
    ),
    "ini-key",
  )?

  # A section key is validated after normalization, so a rejected key reports
  # the lowercase spelling under the `ini-key` kind.
  test.error_kind(ini.encode({s: {a[b: "1"}}), "ini-key")?
  test.eq(encode_message(ini.encode({s: {a[b: "1"}})), "invalid INI key")?
  test.error_kind(ini.encode({s: {: "1"}}), "ini-key")?
  test.error_kind(ini.encode({s: {a]b: "1"}}), "ini-key")?
  test.error_kind(
    ini.encode(
      {
        s: {
          a
      b: "1",
        },
      },
    ),
    "ini-key",
  )?

  # Section names keep their spelling and carry their own kind.
  test.error_kind(ini.encode({a[b: {h: "1"}}), "ini-section")?
  test.eq(encode_message(ini.encode({a[b: {h: "1"}})), "invalid INI section")?
  test.error_kind(ini.encode({: {h: "1"}}), "ini-section")?
  test.error_kind(ini.encode({a]b: {h: "1"}}), "ini-section")?
  test.error_kind(
    ini.encode(
      {
        a
      b: {
        h: "1",
      },
      },
    ),
    "ini-section",
  )?

  # The whole record is collected before any global key is validated, so a
  # section-field rejection outranks a global-key rejection even when the
  # offending global key sorts first.
  test.error_kind(ini.encode({a[b: "1", s: {c: 2}}), "ini-encode")?
  test.eq(
    encode_message(ini.encode({a[b: "1", s: {c: 2}})),
    "INI section values must be strings",
  )?
}

proc test_ini_encode_rejects_non_string_values() [fs, error] {
  # A top-level field must be a global string or a section record.
  test.error_kind(ini.encode({a: 1}), "ini-encode")?
  test.eq(
    encode_message(ini.encode({a: 1})),
    "INI records may contain only global string keys or section records",
  )?
  test.error_kind(ini.encode({a: true}), "ini-encode")?
  test.error_kind(ini.encode({a: null}), "ini-encode")?
  test.error_kind(ini.encode({a: [1, 2]}), "ini-encode")?

  # A map is not a section record.
  test.error_kind(ini.encode({s: map.empty()}), "ini-encode")?
  test.eq(
    encode_message(ini.encode({s: map.empty()})),
    "INI records may contain only global string keys or section records",
  )?

  # A section field must be a string.
  test.error_kind(ini.encode({s: {a: 1}}), "ini-encode")?
  test.eq(encode_message(ini.encode({s: {a: 1}})), "INI section values must be strings")?
  test.error_kind(ini.encode({s: {a: true}}), "ini-encode")?
  test.error_kind(ini.encode({s: {a: null}}), "ini-encode")?
  test.error_kind(ini.encode({s: {a: [1]}}), "ini-encode")?

  # The check runs in the section's key order, so `a` is judged before `c`.
  test.error_kind(ini.encode({s: {c]d: "2", a: 1}}), "ini-encode")?
}

proc test_ini_encode_writes_multiline_values() [fs, error] {
  # Embedded newlines become two-space continuation lines, so the value reads
  # back as the same string.
  test.eq(
    ini.encode(
  {
  a: """hello
world""",
},
)?,
    """a = hello
  world
""",
  )?
  test.eq(
    ini.encode(
  {
  s: {
    message: """hello
world""",
  },
},
)?,
    """[s]
message = hello
  world
""",
  )?
  test.eq(
    ini.encode(
  {
  a: """x

y""",
},
)?,
    """a = x
  
  y
""",
  )?

  # A value that ends in a newline emits a continuation line holding only the
  # indent, and an empty value keeps the key line with an empty right side.
  test.eq(
    ini.encode(
  {
  a: """x
""",
},
)?,
    """a = x
  
""",
  )?
  test.eq(
    ini.encode({a: ""})?,
    """a = 
""",
  )?

  # The last line ends with exactly one newline, and the result re-decodes to
  # the value it was built from.
  test.eq(
    ini.encode({a: "1"})?,
    """a = 1
""",
  )?
  test.eq(
    ini.encode({a: "1"})?.ends_with("""

"""),
    false,
  )?
  let round_trip = ini.decode(
    ini.encode(
  {
  global: "root",
  server: {
    message: """hello
world""",
  },
},
)?,
  )?
  test.eq(round_trip.global, "root")?
  test.eq(
    round_trip.server.message,
    """hello
world""",
  )?
}
