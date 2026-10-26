test test_codec_dispatch_preserves_values_and_rejects_wrong_boundaries {
  assert codec.decode("json", "{\"value\":[1,true,null]}")? == json.decode("{\"value\":[1,true,null]}")?
  assert codec.encode("json", {value: [1, 2]})? == json.encode({value: [1, 2]})?
  assert codec.decode_bytes("yaml", b"value: 42\n")? == yaml.decode("value: 42\n")?
  assert codec.decode_bytes("json", codec.encode_bytes("json", {value: 42})?)? == json.decode("{\"value\":42}")?
  test.error_kind(codec.decode("cbor", ""), "CodecError.UnsupportedFormat")
  test.error_kind(codec.encode("msgpack", 1), "CodecError.UnsupportedFormat")
  test.error_kind(codec.decode("JSON", "null"), "CodecError.UnsupportedFormat")
  test.error_kind(codec.decode("pickle", ""), "CodecError.UnsupportedFormat")
  test.error_kind(codec.encode_bytes("json", 1, canonical: true), "CodecError.Options")
  match codec.decode_bytes("toml", b"a=\xff") {
    Err(CodecError.Encoding {format: format, offset: offset, input_path: input_path, ..}) => {
      assert format == "toml"
      assert offset == 2
      assert input_path == null
    }
    else => return test.fail("expected strict UTF-8 failure")
  }
}

test test_codec_file_writes_encode_before_atomic_no_clobber { |ctx|
  let path = test.temp_path(ctx, name: "value.json")
  codec.write("json", path, {value: 1})
  test.error_kind(codec.write("json", path, {value: 2}), "fs-write")
  assert codec.read("json", path)? == json.decode("{\"value\":1}")?
  let unsupported: Any = p"data"
  test.error_kind(codec.write("json", path, unsupported), "CodecError.UnsupportedValue")
  assert codec.read("json", path)? == json.decode("{\"value\":1}")?
  codec.write("json", path, {value: 2}, overwrite: true)
  assert codec.read("json", path)? == json.decode("{\"value\":2}")?
  path.write_bytes(b"\xff")
  match codec.read("json", path) {
    Err(CodecError.Encoding {input_path: input_path, ..}) => assert input_path == path
    else => return test.fail("expected codec input path")
  }
}
