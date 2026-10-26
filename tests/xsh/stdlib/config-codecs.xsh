test test_yaml_12_scalars_preserve_quoted_types_and_bytes {
  assert yaml.decode("")? == null
  let value = yaml.decode("""name: worker
quoted: "8080"
port: 8080
ratio: 1.0
legacy: yes
empty: null
flags: [true, false]
date: 2026-10-26
data: !!binary AP9h
""")?
  assert json.get(value, ["name"])? == "worker"
  assert json.get(value, ["quoted"])?.require(Str)? == "8080"
  assert json.get(value, ["port"])?.require(Int)? == 8080
  assert json.get(value, ["ratio"])?.require(Float)? == 1.0
  assert json.get(value, ["legacy"])? == "yes"
  assert json.get(value, ["empty"])? == null
  assert json.get(value, ["date"])? == "2026-10-26"
  assert json.get(value, ["flags"])? == [true, false]
  assert json.get(value, ["data"])?.require(Bytes)? == b"\0\xffa"
  let explicit = yaml.decode("""%YAML 1.2
---
flag: !!bool true
text: !!str 42
""")?
  assert json.get(explicit, ["flag"])? == true
  assert json.get(explicit, ["text"])? == "42"
  test.error_kind(yaml.decode("%YAML 1.1\n---\nflag: yes\n"), "CodecError.Syntax")
  test.error_kind(yaml.decode("9223372036854775808"), "CodecError.Overflow")
  test.error_kind(yaml.decode("!custom value"), "CodecError.UnsupportedValue")
  test.error_kind(yaml.decode("value: !custom [1]"), "CodecError.UnsupportedValue")
  test.error_kind(yaml.decode("!!timestamp 2026-10-26"), "CodecError.UnsupportedValue")
  test.error_kind(yaml.decode("1: value"), "CodecError.UnsupportedValue")
  test.error_kind(yaml.decode("? [a, b]\n: value\n"), "CodecError.UnsupportedValue")
}

test test_yaml_aliases_merges_and_duplicate_keys {
  let value = yaml.decode("""base: &base {name: worker}
copy: *base
app: {<<: *base, jobs: 2}
""")?
  assert json.get(value, ["copy", "name"])? == "worker"
  assert json.get(value, ["app", "name"])? == "worker"
  assert json.get(value, ["app", "jobs"])? == 2
  test.error_kind(yaml.decode("a: 1\na: 2\n"), "CodecError.DuplicateKey")
  test.error_kind(yaml.decode("base: &base {a: 1}\napp: {<<: *base, a: 2}\n"), "CodecError.DuplicateKey")
  test.error_kind(yaml.decode("a: &a {x: 1}\nb: &b {x: 2}\napp: {<<: [*a, *b]}\n"), "CodecError.DuplicateKey")
  test.error_kind(yaml.decode("cycle: &cycle [*cycle]"), "CodecError.Syntax")
  test.error_kind(yaml.decode("missing: *missing"), "CodecError.Syntax")
  test.error_kind(yaml.decode("--- 1\n--- 2\n"), "CodecError.Syntax")
}

test test_yaml_encoding_roundtrips_nested_values_and_files { |ctx|
  let empty: List[Int] = []
  let value = {name: "yes", nested: {items: [1, 2], flag: true}, empty: empty, data: b"\0\xffa"}
  let encoded = yaml.encode(value)?
  let decoded = yaml.decode(encoded)?
  assert json.get(decoded, ["name"])? == "yes"
  assert json.get(decoded, ["nested", "items"])? == [1, 2]
  assert json.get(decoded, ["nested", "flag"])? == true
  assert json.get(decoded, ["empty"])? == []
  assert json.get(decoded, ["data"])?.require(Bytes)? == b"\0\xffa"
  let path = test.temp_path(ctx, name: "config.yaml")
  yaml.write(path, value)
  assert json.get(yaml.read(path)?, ["nested", "items"])? == [1, 2]
  let unsupported: Any = p"data"
  test.error_kind(yaml.encode(unsupported), "CodecError.UnsupportedValue")
}

test test_toml_arrays_of_tables_and_explicit_representation_limits {
  assert json.encode(toml.decode("")?)? == "{}"
  let value = toml.decode("""title = "example"
ratio = 1.0
[[services]]
name = "worker"
port = 8080
[[services]]
name = "api"
port = 8081
""")?
  let inline = toml.decode("table = {\n  x = 1, # TOML 1.1\n  nested = {\n    enabled = true,\n  },\n}\n")?
  assert json.get(inline, ["table", "nested", "enabled"])? == true
  assert json.get(value, ["title"])? == "example"
  assert json.get(value, ["ratio"])?.require(Float)? == 1.0
  assert json.get(value, ["services", 0, "port"])? == 8080
  assert json.get(value, ["services", 1, "name"])? == "api"
  test.error_kind(toml.decode("a = 1\na = 2\n"), "CodecError.DuplicateKey")
  test.error_kind(toml.decode("a = 18446744073709551616\n"), "CodecError.Overflow")
  test.error_kind(toml.decode("a = -9223372036854775809\n"), "CodecError.Overflow")
  test.error_kind(toml.decode("a = 1e400\n"), "CodecError.Overflow")
  test.error_kind(toml.decode("when = 2026-10-26T12:34:56.123456789Z\n"), "CodecError.UnsupportedValue")
  test.error_kind(toml.decode("when = 2026-10-26\n"), "CodecError.UnsupportedValue")
  test.error_kind(toml.decode("when = 12:34:56\n"), "CodecError.UnsupportedValue")
  test.error_kind(toml.encode({missing: null}), "CodecError.UnsupportedValue")
  test.error_kind(toml.encode({bytes: b"text"}), "CodecError.UnsupportedValue")
  test.error_kind(toml.encode({values: [null]}), "CodecError.UnsupportedValue")
}

test test_toml_encoding_roundtrips_and_preserves_failed_destinations { |ctx|
  let value = {title: "example", enabled: true, nested: {ports: [80, 443]}, services: [{name: "api"}, {name: "worker"}]}
  let encoded = toml.encode(value)?
  assert json.encode(toml.decode(encoded)?)? == json.encode(value)?
  let path = test.temp_path(ctx, name: "config.toml")
  toml.write(path, value)
  assert json.encode(toml.read(path)?)? == json.encode(value)?
  let original = path.read_text()?
  test.error_kind(toml.write(path, {bad: null}), "CodecError.UnsupportedValue")
  assert path.read_text()? == original
}


test test_configuration_formats_share_values_without_hidden_string_coercion {
  let value = yaml.decode("service: worker\nports: [80, 443]\n")?
  let json_value = json.decode(json.encode(value)?)?
  assert json.get(json_value, ["ports"])? == [80, 443]
  let ini_value = ini.decode("port = 8080\n")?
  test.error_kind(json.get(ini_value, ["port"])?.require(Int), "schema")
  let yaml_string = yaml.decode("port: \"8080\"\n")?
  test.error_kind(json.get(yaml_string, ["port"])?.require(Int), "schema")
  let toml_string = toml.decode("port = \"8080\"\n")?
  test.error_kind(json.get(toml_string, ["port"])?.require(Int), "schema")
}
