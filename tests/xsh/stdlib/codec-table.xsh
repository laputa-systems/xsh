test test_csv_decoding_headers_and_headerless_rows {
  assert csv.decode("name,age\nAda,37\n")? == [{name: "Ada", age: "37"}]
  assert csv.decode("name,age\nAda,37\n", headers: false)? == [["name", "age"], ["Ada", "37"]]
  assert csv.decode("name;age\nAda;37\n", delimiter: ";")? == [{name: "Ada", age: "37"}]
  assert csv.decode("name,city\r\nAda,東京\r\n")? == [{name: "Ada", city: "東京"}]
}

test test_csv_rejects_duplicate_headers_widths_quotes_and_invalid_delimiters {
  test.error_kind(csv.decode("name,name\n"), "CodecError.DuplicateKey")
  test.error_kind(csv.decode("name,name\nAda,37\n"), "CodecError.DuplicateKey")
  test.error_kind(csv.decode("name,age\nAda\n"), "CodecError.Syntax")
  test.error_kind(csv.decode("name\n\"Ada\"tail\n"), "CodecError.Syntax")
  test.error_kind(csv.decode("name\nAda\n", delimiter: ""), "CodecError.Options")
  test.error_kind(csv.decode("name\nAda\n", delimiter: "é"), "CodecError.Options")
  test.error_kind(csv.decode("name\nAda\n", delimiter: "\""), "CodecError.Options")
}

test test_csv_columns_are_ordered_and_exact_for_every_record {
  let rows = [{a: "one", b: "two"}, {a: "three", b: "four"}]
  assert csv.encode(rows, columns: ["b", "a"])? == "b,a\ntwo,one\nfour,three\n"
  assert csv.encode(rows, headers: false, columns: ["b", "a"])? == "two,one\nfour,three\n"
  let reordered = [{a: "one", b: "two"}, {b: "four", a: "three"}]
  assert csv.encode(reordered)? == "a,b\none,two\nthree,four\n"
  assert csv.encode(reordered, headers: false)? == "one,two\nthree,four\n"
  test.error_kind(csv.encode([{a: "one", b: "two"}, {a: "three"}]), "CodecError.UnsupportedValue")
  test.error_kind(csv.encode([{a: "one", b: "two"}, {a: "three", b: "four", c: "five"}]), "CodecError.UnsupportedValue")
  test.error_kind(csv.encode([{a: "one"}], columns: ["a", "b"]), "CodecError.UnsupportedValue")
  test.error_kind(csv.encode([{a: "one", b: "two", c: "three"}], columns: ["a", "b"]), "CodecError.UnsupportedValue")
  test.error_kind(csv.encode(rows, columns: ["a", "a"]), "CodecError.Options")
}

test test_csv_formula_escaping_is_opt_in_and_tsv_uses_its_preset {
  let rows = [{value: "=1+2"}]
  assert csv.encode(rows)? == "value\n=1+2\n"
  assert csv.encode(rows, escape_formulas: true)? == "value\n\"'=1+2\"\n"
  assert codec.decode("tsv", "name\tvalue\nAda\tx\\ty\n")? == [{name: "Ada", value: "x\ty"}]
  assert codec.decode("tsv", "name\tvalue\nAda\t\\N\n")? == [{name: "Ada", value: null}]
  assert codec.encode("tsv", [{name: "Ada", value: "x\ty"}])? == "name\tvalue\nAda\tx\\ty\n"
}

test test_codec_table_dispatch_uses_strict_utf8 {
  test.error_kind(codec.decode_bytes("csv", b"name\n\xff\n"), "CodecError.Encoding")
}

test test_csv_files_preserve_options_and_failed_write_destinations { |ctx|
  let config_path = test.temp_path(ctx, name: "ordered.csv")
  let rows = [{a: "one", b: "two"}]
  csv.write(config_path, rows, headers: false, delimiter: ";", columns: ["b", "a"])?
  assert config_path.read_text()? == "two;one\n"
  assert csv.read(config_path, headers: false, delimiter: ";")? == [["two", "one"]]
  test.error_kind(csv.write(config_path, rows), "fs-write")
  test.error_kind(csv.write(config_path, [{a: "changed"}], columns: ["a", "b"], overwrite: true), "CodecError.UnsupportedValue")
  test.error_kind(csv.write(config_path, rows, delimiter: "\n", overwrite: true), "CodecError.Options")
  assert config_path.read_text()? == "two;one\n"
  csv.write(config_path, rows, overwrite: true)?
  assert csv.read(config_path)? == rows
  assert codec.read("csv", config_path)? == rows
  config_path.write(b"name\n\xff\n")
  match csv.read(config_path) {
    Err(CodecError.Encoding {input_path: actual, ..}) => assert actual == config_path
    else => return test.fail("expected CSV input path")
  }
  let effect = test.expect(ctx, "pure read_csv(config_path: Path) -> Result[Any] { csv.read(config_path) }\n", status: 2)?
  assert "effect" in effect.stderr
}
