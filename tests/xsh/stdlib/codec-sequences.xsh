test test_codec_sequences_recover_json_lines_and_csv_records {
  let lines = codec.decode_all("json", "\n1\nbad\n2\n")? |> collect()
  assert lines.len() == 3
  assert lines[0]? == 1
  assert lines[2]? == 2
  match lines[1] {
    Err(CodecError.Syntax {line, value_path, ..}) => {
      assert line == 3
      assert value_path == "[2]"
    }
    else => return test.fail("expected an indexed JSON line error")
  }
  let rows = codec.decode_all("csv", "a,b\n1,2\nwrong\n3,4\n")? |> collect()
  assert rows.len() == 3
  assert rows[0]? == {a: "1", b: "2"}
  test.error_kind(rows[1], "CodecError.Syntax")
  assert rows[2]? == {a: "3", b: "4"}
}

test test_codec_sequences_stop_yaml_binary_and_header_failures {
  let yaml_items = codec.decode_all("yaml", "--- 1\n--- [broken\n--- 2\n")? |> collect()
  assert yaml_items.len() == 2
  assert yaml_items[0]? == 1
  test.error_kind(yaml_items[1], "CodecError.Syntax")
  let headers = codec.decode_all("csv", "a,a\n1,2\n3,4\n")? |> collect()
  assert headers.len() == 1
  test.error_kind(headers[0], "CodecError.DuplicateKey")
  for format in ["cbor", "msgpack"] {
    let prefix = codec.encode_all_bytes(format, [1, 2])?
    let suffix = if format == "cbor" { b"\xff" } else { b"\xc1" }
    let items = codec.decode_all_bytes(format, bytes.concat([prefix, suffix]))? |> collect()
    assert items.len() == 3
    assert items[0]? == 1
    assert items[1]? == 2
    test.error_kind(items[2], "CodecError.Syntax")
  }
}

test test_codec_sequences_encode_one_table_header_and_preserve_keysets {
  let rows: List[Any] = [{a: "1", b: "2"}, {a: "3", b: "4"}]
  assert codec.encode_all("csv", rows)? == "a,b\n1,2\n3,4\n"
  assert codec.encode_all("tsv", rows)? == "a\tb\n1\t2\n3\t4\n"
  test.error_kind(codec.encode_all("csv", [{a: 1, b: 2}, {a: 3}]), "CodecError.UnsupportedValue")
  assert codec.encode_all("json", [1, true, null])? == "1\ntrue\nnull\n"
  for format in ["json", "yaml", "cbor", "msgpack"] {
    let expected: List[Any] = [1, 2, 3]
    let encoded = codec.encode_all_bytes(format, expected)?
    let decoded = codec.decode_all_bytes(format, encoded)? |> map { |item| item? } |> collect()
    assert decoded == expected
  }
}

test test_codec_sequences_own_independent_file_reads { |ctx|
  let root = test.temp_dir(ctx, name: "codec-sequences")?
  let input = fp"{root}/values.jsonl"
  input.write("1\n2\n3\n")
  let first = codec.read_all("json", input)? |> take(1) |> collect()
  assert first.len() == 1
  assert first[0]? == 1
  let second = codec.read_all("json", input)? |> collect()
  assert second.len() == 3
  assert second[2]? == 3
  test.error_kind(codec.decode_all("toml", "a=1"), "CodecError.UnsupportedFormat")
  test.error_kind(codec.decode_all_bytes("ini", b"a=1"), "CodecError.UnsupportedFormat")
  test.error_kind(codec.encode_all("toml", [1]), "CodecError.UnsupportedFormat")
}
