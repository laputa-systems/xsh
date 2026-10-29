test test_codec_unknown_file_format_keeps_the_input_path { |ctx|
  let config_path = test.temp_path(ctx, name: "missing.data")
  match codec.read("unavailable", config_path) {
    Err(CodecError.UnsupportedFormat {input_path: actual, ..}) => assert actual == config_path
    else => return test.fail("expected unsupported format before opening the file")
  }
}

test test_csv_invalid_file_options_keep_the_input_path { |ctx|
  let config_path = test.temp_path(ctx, name: "missing.csv")
  match csv.read(config_path, delimiter: "\n") {
    Err(CodecError.Options {input_path: actual, ..}) => assert actual == config_path
    else => return test.fail("expected invalid options before opening the file")
  }
}

test test_codec_file_sequences_reject_formats_before_opening { |ctx|
  let config_path = test.temp_path(ctx, name: "missing.ini")
  for format in ["unavailable", "ini", "toml"] {
    match codec.read_all(format, config_path) {
      Err(CodecError.UnsupportedFormat {input_path: actual, ..}) => assert actual == config_path
      else => return test.fail("expected sequence format rejection before opening the file")
    }
  }
}

test test_codec_sequence_encoding_never_resumes_a_producer { |ctx|
  let output = test.expect(ctx, r"""stream numbers() [io] -> Stream[Int] {
  print "pulled"
  yield 1
}
let value: Any = numbers()
match codec.encode_all("json", [value]) {
  Err(CodecError.UnsupportedValue {..}) => {}
  else => assert false
}
print "checked"
""", status: 0)?
  assert output.stdout == "checked\n"
}

test test_codec_files_roundtrip_each_format_and_preserve_failed_destinations { |ctx|
  let record = {name: "worker", port: "42"}
  for format in ["json", "yaml", "toml", "ini", "csv", "tsv"] {
    let config_path = test.temp_path(ctx, name: f"roundtrip-{format}.data")
    let value: Any = if format == "csv" or format == "tsv" { [record] } else { record }
    codec.write(format, config_path, value)?
    assert codec.read(format, config_path)? == value
    let original = config_path.read_bytes()?
    test.error_kind(codec.write(format, config_path, value), "fs-write")
    let unsupported: Any = p"data"
    test.error_kind(codec.write(format, config_path, unsupported, overwrite: true), "CodecError.UnsupportedValue")
    assert config_path.read_bytes()? == original
  }
}

test test_codec_file_sequences_preserve_non_utf8_paths { |ctx|
  if system.uname()?.sysname == "Darwin" {
    test.skip("macOS filesystems reject non-UTF-8 filenames")
    return
  }
  let root = test.temp_dir(ctx, name: "codec-byte-path")?
  let input = bytes.concat([root.bytes(), b"/input\xff.jsonl"]) as Path
  input.write("bad\n2\n")
  let values = codec.read_all("json", input)? |> collect()
  assert values.len() == 2
  match values[0] {
    Err(CodecError.Syntax {input_path: actual, ..}) => {
      assert actual == input
    }
    else => return test.fail("expected the byte-safe input path")
  }
  assert values[1]? == 2
}

test test_codec_file_sequence_host_read_failure_names_the_path { |ctx|
  let directory = test.temp_dir(ctx, name: "codec-directory-read")?
  let consumed: Result[Unit] = try {
    for item in codec.read_all("json", directory)? { let _ = item? }
  }
  match consumed {
    Err(error) => assert directory.display() in error.message
    else => return test.fail("expected a host read failure from the directory")
  }
}

test test_codec_file_boundaries_follow_existing_symlink_policy { |ctx|
  let root = test.temp_dir(ctx, name: "codec-file-links")?
  let target = fp"{root}/target.json"
  let link = fp"{root}/input.json"
  codec.write("json", target, 1)?
  link.symlink(to: target)
  assert codec.read("json", link)? == 1
  let values = codec.read_all("json", link)? |> collect()
  assert values.len() == 1
  assert values[0]? == 1
  test.error_kind(codec.write("json", link, 2), "fs-write")
  assert codec.read("json", target)? == 1
  codec.write("json", link, 2, overwrite: true)?
  assert link.readlink()? == target
  assert codec.read("json", target)? == 2
  let missing = fp"{root}/missing.json"
  let dangling = fp"{root}/dangling.json"
  dangling.symlink(to: missing)
  test.error_kind(codec.write("json", dangling, 3), "fs-write")
  assert ! missing.exists()?
  test.error_kind(codec.read_all("json", dangling), "fs-read")
  assert dangling.readlink()? == missing
}

test test_codec_file_writes_preserve_read_only_destinations { |ctx|
  if unix.id()?.euid == 0 {
    test.skip("root bypasses file permission checks")
    return
  }
  let input = test.temp_path(ctx, name: "codec-read-only.json")
  codec.write("json", input, {value: 1})?
  input.chmod(0o400)
  defer input.chmod(0o600)
  assert codec.read("json", input)? == {value: 1}
  test.error_kind(codec.write("json", input, {value: 2}, overwrite: true), "fs-write")
  assert codec.read("json", input)? == {value: 1}
}
