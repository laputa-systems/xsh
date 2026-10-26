type PortField[T] = {port: T}
type ValueField[T] = {value: T}
type FlagField = {flag: Bool}
type MissingField = {missing: Int}
type ServerValue = {port: UInt, name: Str}
type ServerField = {server: ServerValue}
type TsvMarkerRow = {missing: Int?, text: Str}
type NamedPorts = {ports: Map[Str, UInt]}
type WebPort = {web: UInt}
type PortGroup = {ports: WebPort}
type TableRow = {name: Str, port: Int}
type LowCount = Int range 0..=10
type HighCount = Int range 5..=15
type Service = {name: Str, port: UInt, enabled: Bool, ratio: Float, spare: Int?}

test test_codec_typed_lexical_primitives_and_preserved_extras {
  let value = codec.decode_as("ini", "name=worker\nport=8080\nenabled=true\nratio=1e2\nspare=\nextra=007\n", Service)?
  assert value.name == "worker"
  assert value.port == 8080
  assert value.enabled
  assert value.ratio == 100.0
  assert value.spare == null
  assert json.get(value, ["extra"])? == "007"
  test.error_kind(ini.decode("port=8080")?.require(PortField[Int]), "schema")
  let rows = codec.decode_as("csv", "name,port,enabled,ratio,spare\nworker,8080,true,1.5,\n", List[Service])?
  assert rows[0].port == 8080
  assert rows[0].spare == null
  let tsv = codec.decode_as("tsv", "name\tport\nworker\t42\n", List[TableRow])?
  assert tsv[0].port == 42
}

test test_codec_typed_lexical_rejects_repairs_and_ambiguity {
  for text in ["port=0x10", "port=1_000", "port=9223372036854775808"] {
    test.error_kind(codec.decode_as("ini", text, PortField[Int]), "CodecError.Schema")
  }
  test.error_kind(codec.decode_as("csv", "port\n\"  1  \"\n", List[PortField[Int]]), "CodecError.Schema")
  test.error_kind(codec.decode_as("ini", "flag=yes", FlagField), "CodecError.Schema")
  test.error_kind(codec.decode_as("ini", "value=NaN", ValueField[Float]), "CodecError.Schema")
  test.error_kind(codec.decode_as("ini", "value=1", ValueField[Union[Int, Float]]), "CodecError.Schema")
  let strings = codec.decode_as("ini", "value=1", ValueField[Union[Str, Int]])?
  assert json.get(strings, ["value"])? == "1"
  let integer = codec.decode_as("ini", "value=7", ValueField[Union[LowCount, HighCount]])?
  assert json.get(integer, ["value"])? == 7
  test.error_kind(codec.decode_as("json", "1", Int), "CodecError.UnsupportedFormat")
  test.error_kind(codec.decode_as("ini", "other=value", MissingField), "CodecError.Schema")
}

test test_codec_typed_lexical_nested_schema_and_files { |ctx|
  let value = codec.decode_as("ini", "[server]\nport=42\nname=worker\n", ServerField)?
  assert value.server.port == 42
  let config_path = test.temp_path(ctx, name: "typed.ini")
  config_path.write("port=42\n")
  let read = codec.read_as("ini", config_path, PortField[UInt])?
  assert read.port == 42
  let unknown = codec.decode_as("ini", "value=007", Any)?
  assert json.get(unknown, ["value"])? == "007"
  test.error_kind(codec.decode_as("csv", "value\n\"[1,2]\"\n", List[ValueField[List[Int]]]), "CodecError.Schema")
}

enum ServiceState: Str { Ready = "ready", Missing = "missing" }
type StatePort = {state: ServiceState, port: UInt}

test test_codec_typed_lexical_admission_owns_refinements_and_wire_enums {
  let value = codec.decode_as("ini", "state=ready\nport=42\n", StatePort)?
  assert value.state == Ready
  test.error_kind(codec.decode_as("ini", "state=unknown\nport=42\n", StatePort), "CodecError.Schema")
  test.error_kind(codec.decode_as("ini", "state=ready\nport=-1\n", StatePort), "CodecError.Schema")
  let rows = codec.decode_as("csv", "port\n42\n", NonEmpty[PortField[UInt]])?
  assert rows.first().port == 42
  test.error_kind(codec.decode_as("csv", "port\n", NonEmpty[PortField[UInt]]), "CodecError.Schema")
}

test test_codec_typed_lexical_schema_operands_and_file_effects_are_explicit { |ctx|
  let missing = test.expect(ctx, "let value: Result[Int] = codec.decode_as(\"ini\", \"x=1\")\n", status: 2)?
  assert "codec" in missing.stderr
  let projection = test.expect(ctx, "let decode = codec.decode_as\n", status: 2)?
  assert "must be called" in projection.stderr
  let effect = test.expect(ctx, "type Port = {port: Int}\npure read(config_path: Path) { codec.read_as(\"ini\", config_path, Port) }\n", status: 2)?
  assert "effect" in effect.stderr
}

test test_codec_typed_tsv_preserves_native_null_and_escaped_markers {
  let rows = codec.decode_as("tsv", "missing\ttext\n\\N\t\\\\N\n", List[TsvMarkerRow])?
  assert rows[0].missing == null
  assert rows[0].text == "\\N"
  test.error_kind(codec.decode_as("tsv", "value\n\\N\n", List[ValueField[Int]]), "CodecError.Schema")
}

test test_codec_typed_value_operands_evaluate_left_to_right_once { |ctx|
  let result = test.expect(ctx, r"""type Port = {port: Int}
proc format() [io] -> Str { print "format"; return "ini" }
proc text() [io] -> Str { print "input"; return "port=42" }
let value = codec.decode_as(format: format(), text: text(), Port)?
print value.port
""", status: 0)?
  assert result.stdout == "format\ninput\n42\n"
  let reordered = test.expect(ctx, "type Port = {port: Int}\nlet value = codec.decode_as(text: \"port=1\", format: \"ini\", Port)\n", status: 2)?
  assert "declared order" in reordered.stderr
  let duplicate = test.expect(ctx, "type Port = {port: Int}\nlet value = codec.decode_as(format: \"ini\", format: \"ini\", Port)\n", status: 2)?
  assert "schema" in duplicate.stderr
  let spread = test.expect(ctx, "type Port = {port: Int}\nlet value = codec.decode_as(@[\"ini\", \"port=1\"], Port)\n", status: 2)?
  assert "spreads" in spread.stderr
  let dynamic = test.expect(ctx, "let target = 1\nlet value = codec.decode_as(\"ini\", \"port=1\", target)\n", status: 2)?
  assert "target" in dynamic.stderr
}

test test_codec_typed_maps_and_failure_locations { |ctx|
  let values = codec.decode_as("ini", "one=1\ntwo=2\n", Map[Str, Int])?
  assert values.get("one")? == 1
  let nested = codec.decode_as("ini", "[ports]\nweb=80\napi=443\n", NamedPorts)?
  assert nested.ports.get("api")? == 443
  let config_path = test.temp_path(ctx, name: "invalid-typed.ini")
  config_path.write("[ports]\nweb=invalid\n")
  match codec.read_as("ini", config_path, PortGroup) {
    Err(CodecError.Schema {input_path: actual, value_path: location, ..}) => {
      assert actual == config_path
      assert location == "ports.web"
    }
    _ => assert false
  }
}

test test_codec_typed_imported_aliases_keep_nominal_identity { |ctx|
  let root = test.temp_dir(ctx, name: "typed-codec-owner")?
  fp"{root}/model.xsh".write_atomic(r"""##! Typed lexical model.
nominal type Identity = PortField[UInt]
## Export the declaring identity through an alias.
export type Service = Identity
## Read the admitted identity.
export pure port(service: Service) -> UInt { service.port }
""")
  let output = test.expect(ctx, r"""use model as m
nominal type Identity = {port: Str}
let value = codec.decode_as("ini", "port=42", m.Service)?
print m.port(value)
""", status: 0, args: [], env: {XSH_MODULE_PATH: root})?
  assert output.stdout == "42\n"
}
