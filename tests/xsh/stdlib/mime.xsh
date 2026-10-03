# The looked-up entry, or a marker record when the lookup missed.
#
# Every lookup assertion goes through this so a miss reports the marker media
# type instead of failing on a field access against an absent value.
pure mime_entry(info: MimeInfo?) -> MimeInfo {
  info ?? missing_info()
}

pure missing_info() -> MimeInfo {
  {mime: "missing", exts: ["missing"]}
}

# The `type` field of a successful parse, or `rejected` when it was refused.
pure parsed_type(result: Result[MimeParse]) -> Str {
  if let Ok(parsed) = result {
    parsed.type
  } else {
    "rejected"
  }
}

# A parse's parameter map, empty when the parse was refused.
pure parsed_params(result: Result[MimeParse]) -> Map[Str] {
  if let Ok(parsed) = result {
    parsed.params
  } else {
    {}
  }
}

test test_mime_lookup_and_parse {
  let info = mime.lookup_ext("tar.gz") ?? {mime: "missing", exts: ["missing"]}
  assert info.mime == "application/tar+gzip"
  assert info.exts[0] == "tar.gz"

  assert mime.lookup_path(p"archive.tar.gz")?.mime == "application/tar+gzip"
  assert mime.lookup_ext("definitelymissingxsh") == null
  assert mime.lookup_path(p"no-extension") == null
  let parsed = mime.parse("Text/Plain; Charset=UTF-8")?
  assert parsed.type == "text/plain"
  assert (parsed.params.get("charset") ?? "") == "UTF-8"
  test.error_kind(mime.parse("not a media type"), "mime-parse")?
}

test test_mime_lookup_ext_normalizes_the_query_spelling {
  # Leading dots are dropped and ASCII case is folded before the table is
  # consulted, so these four spellings are one key.
  for spelling in ["gz", ".gz", "GZ", "..GZ"] {
    assert mime_entry(mime.lookup_ext(spelling) ?? missing_info()).mime == "application/gzip"
  }

  # The reported extension list is the row's own list in table order, not the
  # spelling that was asked for.
  let gzip = mime_entry(mime.lookup_ext(".GZ"))
  assert gzip.mime == "application/gzip"
  assert gzip.exts.join(",") == "gz"

  let plain = mime_entry(mime.lookup_ext("TXT"))
  assert plain.mime == "text/plain"
  assert plain.exts.join(",") == "txt,text,log"

  let jpeg = mime_entry(mime.lookup_ext("jpeg"))
  assert jpeg.mime == "image/jpeg"
  assert jpeg.exts.join(",") == "jpg,jpeg"
}

test test_mime_lookup_ext_rejects_unusable_extensions {
  # An empty query, and a query that normalization empties, are not lookups.
  assert mime.lookup_ext("") == null
  assert mime.lookup_ext(".") == null
  assert mime.lookup_ext("...") == null

  # The query is not trimmed and is not a glob: whitespace and inner dots are
  # ordinary characters that no extension key carries.
  assert mime.lookup_ext(" gz") == null
  assert mime.lookup_ext("gz ") == null
  assert mime.lookup_ext("a.b") == null
  assert mime.lookup_ext("tar.gz.tgz") == null

  # A well-formed extension that no row carries.
  assert mime.lookup_ext("definitelymissingxsh") == null
}

test test_mime_lookup_path_tries_longest_suffix_first {
  # `tar.gz` is a table key in its own right, so the compound suffix of the
  # same name wins over the `gz` suffix the name also ends with.
  let compound = mime.lookup_path(p"archive.tar.gz") ?? missing_info()
  assert compound.mime == "application/tar+gzip"
  assert compound.exts.join(",") == "tar.gz,tgz"

  let plain = mime.lookup_path(p"archive.gz") ?? missing_info()
  assert plain.mime == "application/gzip"

  # Suffix selection is case-insensitive, and `tgz` is a second spelling of
  # the compound row.
  assert (mime.lookup_path(p"dir/Archive.TAR.GZ") ?? missing_info()).mime == "application/tar+gzip"
  assert (mime.lookup_path(p"a/b/package.tgz") ?? missing_info()).mime == "application/tar+gzip"

  # Only suffixes that start right after a dot are candidates, so a name that
  # ends in an unknown compound suffix does not fall back to a shorter one.
  assert (mime.lookup_path(p"archive.tar.gz.bak") ?? missing_info()).mime == "missing"
}

test test_mime_lookup_path_uses_only_the_final_component {
  # A dot in a directory name is not a suffix of the file.
  assert (mime.lookup_path(p"dir.d/file") ?? missing_info()).mime == "missing"
  assert (mime.lookup_path(p"dir.d/file.txt") ?? missing_info()).mime == "text/plain"

  # A path with no extension at all, and one whose extension no row carries.
  assert mime.lookup_path(p"noext") == null
  assert mime.lookup_path(p"a/b/README") == null
  assert mime.lookup_path(p"dir/") == null

  # A dot that ends the name, and a leading dot, offer no usable suffix.
  assert mime.lookup_path(p"file.") == null
  assert mime.lookup_path(p".hidden") == null
  assert mime.lookup_path(p"..") == null
}

test test_mime_parse_lowercases_the_type_and_parameter_names {
  # The `type/subtype` field is ASCII-lowercased.
  assert parsed_type(mime.parse("TEXT/PLAIN")) == "text/plain"
  assert parsed_type(mime.parse("Application/Vnd.Demo+Json")) == "application/vnd.demo+json"

  # Parameter names are ASCII-lowercased; parameter values keep their case.
  let parsed = parsed_params(mime.parse("text/plain; Charset=UTF-8; NAME=\"A B\""))
  assert (parsed.get("charset") ?? "absent") == "UTF-8"
  assert (parsed.get("name") ?? "absent") == "A B"
  assert (parsed.get("NAME") ?? "absent") == "absent"
}

test test_mime_parse_splits_on_semicolons_before_interpreting_quotes {
  # The split happens first, so a semicolon inside a quoted value ends the
  # parameter and leaves an unterminated quote behind.
  assert parsed_type(mime.parse("text/plain; name=\"a;b\"")) == "rejected"
  test.error_kind(mime.parse("text/plain; name=\"a;b\""), "mime-parse")?

  # The other consequence of splitting first: a value is quoted only when its
  # own part starts with a quote.
  assert (parsed_params(mime.parse("text/plain; a=1; b=2")).get("b") ?? "absent") == "2"
  assert (parsed_params(mime.parse("text/plain; a=\"q\"")).get("a") ?? "absent") == "q"

  # Empty parts between semicolons are skipped, and semicolons alone are not
  # parameters.
  assert parsed_type(mime.parse("text/plain;")) == "text/plain"
  assert parsed_type(mime.parse("text/plain;;")) == "text/plain"
  assert (parsed_params(mime.parse("text/plain;;")).get("a") ?? "absent") == "absent"
}

test test_mime_parse_keeps_the_last_repeated_parameter {
  # A repeated name replaces the earlier value, and the names collide only
  # after ASCII case folding.
  assert (parsed_params(mime.parse("text/plain; a=1; a=2")).get("a") ?? "absent") == "2"
  assert (parsed_params(mime.parse("text/plain; a=1; A=2")).get("a") ?? "absent") == "2"
  assert (parsed_params(mime.parse("text/plain; a=1; b=2; a=3")).get("b") ?? "absent") == "2"
}

test test_mime_parse_reads_quoted_parameter_values {
  # The outer quotes are removed; a backslash keeps the next character
  # literally, so an escaped quote or backslash survives as data.
  assert (parsed_params(mime.parse("text/plain; name=\"a b\"")).get("name") ?? "absent") == "a b"
  assert (parsed_params(mime.parse("text/plain; name=\"a\\\"b\"")).get("name") ?? "absent") == "a\"b"
  assert (parsed_params(mime.parse("text/plain; name=\"a\\\\b\"")).get("name") ?? "absent") == "a\\b"

  # An empty quoted value is a value; the parameter is not dropped.
  assert (parsed_params(mime.parse("text/plain; name=\"\"")).get("name") ?? "absent") == ""

  # Whitespace around the name and the value is trimmed before either is
  # interpreted, so a quoted value need not start at the `=`.
  assert (parsed_params(mime.parse("text/plain ;  charset = UTF-8 ")).get("charset") ?? "absent") == "UTF-8"
  assert (parsed_params(mime.parse("text/plain; name= \"a b\"")).get("name") ?? "absent") == "a b"
}

test test_mime_parse_rejects_invalid_media_types {
  # The type field must be a non-empty `token/token` pair.
  for value in [
    "",
    "text",
    "/",
    "/plain",
    "not a media type",
    "*/*",
    "text/*",
    "text /plain",
  ] {
    assert parsed_type(mime.parse(value)) == "rejected"
  }

  test.error_kind(mime.parse(""), "mime-parse")?
}

test test_mime_parse_rejects_malformed_parameters {
  # A parameter must be `name=value` with a token name and a token or quoted
  # value, and nothing may follow the closing quote.
  for value in [
    "text/plain; bad",
    "text/plain; =v",
    "text/plain; a=",
    "text/plain; a=b c",
    "text/plain; a=~b",
    "text/plain; b~=c",
    "text/plain; name=\"open",
    "text/plain; name=open\"",
    "text/plain; name=\"a\"b",
    "a b/c",
  ] {
    assert parsed_type(mime.parse(value)) == "rejected"
  }

  test.error_kind(mime.parse("text/plain; bad"), "mime-parse")?

  # One malformed parameter rejects the whole value rather than dropping the
  # parameter.
  assert parsed_type(mime.parse("text/plain; a=1; bad; b=2")) == "rejected"
}
