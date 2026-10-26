test test_ini_decode_encode_and_files { |ctx|
  let config = ini.decode("""Global: root
[server]
Host = example.test
message = hello
  world
""")?
  assert config.Global == "root"
  assert json.get(config, ["server", "Host"])? == "example.test"
  assert json.get(config, ["server", "message"])? == "hello\nworld"
  let encoded = ini.encode({Global: config.Global, server: {Host: json.get(config, ["server", "Host"])?, message: json.get(config, ["server", "message"])?}})?
  assert "Global = root" in encoded
  assert "Host = example.test" in encoded
  assert json.encode(ini.decode(encoded)?)? == json.encode(config)?
  let path = test.temp_path(ctx, name: "app.ini")
  ini.write(path, {Global: "root", server: {Host: "example.test"}})
  assert json.get(ini.read(path)?, ["server", "Host"])? == "example.test"
  test.error_kind(ini.write(path, {Global: "again"}, overwrite: false), "ini-write")
  test.error_kind(ini.write(path, {Global: 1}, overwrite: false), "CodecError.UnsupportedValue")
  assert ini.read(path)?.Global == "root"
  ini.write(path, {Global: "replaced"})
  assert ini.read(path)?.Global == "replaced"
}

test test_ini_common_dialect_preserves_case_quotes_comments_and_strings {
  let config = ini.decode("""Host = first
host = second
port = 8080
truth = true
quoted = "  value ; # quoted  "
inline = plain ; comment
hash = plain # comment
semicolon = one;two
color = #ff0000
[Section]
Name = 'worker'
[Section]
Jobs = 2
""")?
  assert config.Host == "first"
  assert config.host == "second"
  assert config.port.require(Str)? == "8080"
  assert config.truth.require(Str)? == "true"
  test.error_kind(config.port.require(Int), "schema")
  assert config.quoted == "  value ; # quoted  "
  assert config.inline == "plain"
  assert config.hash == "plain"
  assert config.semicolon == "one;two"
  assert config.color == "#ff0000"
  assert json.get(config, ["Section", "Name"])? == "worker"
  assert json.get(config, ["Section", "Jobs"])? == "2"
  assert json.get(config, ["section"], null) == null
}

test test_ini_duplicate_and_conflicting_names_fail {
  test.error_kind(ini.decode("a = first\na = second\n"), "CodecError.DuplicateKey")
  test.error_kind(ini.decode("[s]\na = first\na = second\n"), "CodecError.DuplicateKey")
  test.error_kind(ini.decode("[s]\na = first\n[s]\na = second\n"), "CodecError.DuplicateKey")
  test.error_kind(ini.decode("server = global\n[server]\nhost = worker\n"), "CodecError.DuplicateKey")
  test.error_kind(ini.decode("[broken"), "CodecError.Syntax")
  test.error_kind(ini.decode("bare-key\n"), "CodecError.Syntax")
}

test test_ini_encode_orders_globals_then_sections_and_preserves_names {
  assert ini.encode({})? == ""
  assert ini.encode({b: "2", a: "1"})? == "a = 1\nb = 2\n"
  assert ini.encode({Global: "root", server: {Host: "worker"}})? == "Global = root\n\n[server]\nHost = worker\n"
  assert ini.encode({s: {Host: "1", host: "2"}})? == "[s]\nHost = 1\nhost = 2\n"
  assert ini.encode({s: {host: "2", Host: "1"}})? == "[s]\nHost = 1\nhost = 2\n"
  assert ini.encode({s2: {b: "2"}, s1: {a: "1"}})? == "[s1]\na = 1\n\n[s2]\nb = 2\n"
  assert ini.encode({s: {}})? == "[s]\n"
}

test test_ini_encode_quotes_and_continuations_roundtrip {
  let config = {Empty: "", leading: " space", comment: "x ; y", quote: "\"quoted\"", path: "back\\slash", section: {message: "hello\nworld", blanks: "x\n\ny"}}
  let encoded = ini.encode(config)?
  assert json.encode(ini.decode(encoded)?)? == json.encode(config)?
  assert "leading = \" space\"" in encoded
  assert "comment = \"x ; y\"" in encoded
  assert json.get(ini.decode("[s]\nmessage = hello\n    world\n")?, ["s", "message"])? == "hello\nworld"
  test.error_kind(ini.encode({trailing: "x\n"}), "CodecError.UnsupportedValue")
}

test test_ini_encode_rejects_unrepresentable_names_and_values {
  test.error_kind(ini.encode({"": "x"}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({"a=b": "x"}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({"a:b": "x"}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({"[name": "x"}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({"a\nb": "x"}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({a: 1}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({a: true}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({a: null}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({a: [1]}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({s: {a: 1}}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({s: {a: true}}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({s: {a: null}}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({s: {a: b"bytes"}}), "CodecError.UnsupportedValue")
  test.error_kind(ini.encode({s: {nested: {a: "value"}}}), "CodecError.UnsupportedValue")
  let accepted = {s: {"a[b": "1"}, "a]b": {name: "worker"}}
  assert json.encode(ini.decode(ini.encode(accepted)?)?)? == json.encode(accepted)?
}
