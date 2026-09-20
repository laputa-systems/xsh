# The looked-up entry, or a marker record when the lookup missed.
#
# Every lookup assertion goes through this so a miss reports the marker media
# type instead of failing on a field access against an absent value.
pure mime_entry(info: MimeInfo?) -> MimeInfo {
  return info ?? missing_info()
}

pure missing_info() -> MimeInfo {
  return {mime: "missing", exts: ["missing"]}
}


# `lookup_path` reports its declared optional type but keeps the baseline
# runtime boundary of `Ok`/`Err`, so `??` is the one form that observes both
# spellings correctly.
# The `type` field of a successful parse, or `rejected` when it was refused.
pure parsed_type(result: Result[MimeParse]) -> Str {
  match result {
    Ok(parsed) => return parsed.type
    Err(_) => return "rejected"
  }
}

# A parse's parameter map, empty when the parse was refused.
pure parsed_params(result: Result[MimeParse]) -> Map[Str] {
  match result {
    Ok(parsed) => return parsed.params
    Err(_) => return map.empty()
  }
}

proc test_mime_lookup_and_parse() [fs, error] {
  let info = mime.lookup_ext("tar.gz") ?? {mime: "missing", exts: ["missing"]}
  test.eq(info.mime, "application/tar+gzip")?
  test.eq(info.exts[0], "tar.gz")?
  # `lookup_path` keeps the baseline runtime boundary: a hit is `Ok`, a miss is
  # an error, even though the declared result type is a plain optional.
  # and a present one is read with `?.` or `??`, never with `?`.
  test.eq(mime.lookup_path(p"archive.tar.gz")?.mime, "application/tar+gzip")?
  test.eq(mime.lookup_ext("definitelymissingxsh"), null)?
  let parsed = mime.parse("Text/Plain; Charset=UTF-8")?
  test.eq(parsed.type, "text/plain")?
  test.eq(parsed.params.get("charset", ""), "UTF-8")?
  test.error_kind(mime.parse("not a media type"), "mime-parse")?
}

proc test_mime_lookup_ext_normalizes_the_query_spelling() [fs, error] {
  # Leading dots are dropped and ASCII case is folded before the table is
  # consulted, so these four spellings are one key.
  for spelling in ["gz", ".gz", "GZ", "..GZ"] {
    test.eq(mime_entry(mime.lookup_ext(spelling) ?? missing_info()).mime, "application/gzip")?
  }

  # The reported extension list is the row's own list in table order, not the
  # spelling that was asked for.
  let gzip = mime_entry(mime.lookup_ext(".GZ"))
  test.eq(gzip.mime, "application/gzip")?
  test.eq(gzip.exts.join(","), "gz")?

  let plain = mime_entry(mime.lookup_ext("TXT"))
  test.eq(plain.mime, "text/plain")?
  test.eq(plain.exts.join(","), "txt,text,log")?

  let jpeg = mime_entry(mime.lookup_ext("jpeg"))
  test.eq(jpeg.mime, "image/jpeg")?
  test.eq(jpeg.exts.join(","), "jpg,jpeg")?
}

proc test_mime_lookup_ext_rejects_unusable_extensions() [fs, error] {
  # An empty query, and a query that normalization empties, are not lookups.
  test.eq(mime.lookup_ext(""), null)?
  test.eq(mime.lookup_ext("."), null)?
  test.eq(mime.lookup_ext("..."), null)?

  # The query is not trimmed and is not a glob: whitespace and inner dots are
  # ordinary characters that no extension key carries.
  test.eq(mime.lookup_ext(" gz"), null)?
  test.eq(mime.lookup_ext("gz "), null)?
  test.eq(mime.lookup_ext("a.b"), null)?
  test.eq(mime.lookup_ext("tar.gz.tgz"), null)?

  # A well-formed extension that no row carries.
  test.eq(mime.lookup_ext("definitelymissingxsh"), null)?
}

proc test_mime_lookup_path_tries_longest_suffix_first() [fs, error] {
  # `tar.gz` is a table key in its own right, so the compound suffix of the
  # same name wins over the `gz` suffix the name also ends with.
  let compound = mime.lookup_path(p"archive.tar.gz") ?? missing_info()
  test.eq(compound.mime, "application/tar+gzip")?
  test.eq(compound.exts.join(","), "tar.gz,tgz")?

  let plain = mime.lookup_path(p"archive.gz") ?? missing_info()
  test.eq(plain.mime, "application/gzip")?

  # Suffix selection is case-insensitive, and `tgz` is a second spelling of
  # the compound row.
  test.eq((mime.lookup_path(p"dir/Archive.TAR.GZ") ?? missing_info()).mime, "application/tar+gzip")?
  test.eq((mime.lookup_path(p"a/b/package.tgz") ?? missing_info()).mime, "application/tar+gzip")?

  # Only suffixes that start right after a dot are candidates, so a name that
  # ends in an unknown compound suffix does not fall back to a shorter one.
  test.eq((mime.lookup_path(p"archive.tar.gz.bak") ?? missing_info()).mime, "missing")?
}

proc test_mime_lookup_path_uses_only_the_final_component() [fs, error] {
  # A dot in a directory name is not a suffix of the file.
  test.eq((mime.lookup_path(p"dir.d/file") ?? missing_info()).mime, "missing")?
  test.eq((mime.lookup_path(p"dir.d/file.txt") ?? missing_info()).mime, "text/plain")?

  # A path with no extension at all, and one whose extension no row carries.
  test.error_kind(mime.lookup_path(p"noext"), "mime-lookup")?
  test.error_kind(mime.lookup_path(p"a/b/README"), "mime-lookup")?
  test.error_kind(mime.lookup_path(p"dir/"), "mime-lookup")?

  # A dot that ends the name, and a leading dot, offer no usable suffix.
  test.error_kind(mime.lookup_path(p"file."), "mime-lookup")?
  test.error_kind(mime.lookup_path(p".hidden"), "mime-lookup")?
  test.error_kind(mime.lookup_path(p".."), "mime-lookup")?
}

proc test_mime_parse_lowercases_the_type_and_parameter_names() [error] {
  # The `type/subtype` field is ASCII-lowercased.
  test.eq(parsed_type(mime.parse("TEXT/PLAIN")), "text/plain")?
  test.eq(parsed_type(mime.parse("Application/Vnd.Demo+Json")), "application/vnd.demo+json")?

  # Parameter names are ASCII-lowercased; parameter values keep their case.
  let parsed = parsed_params(mime.parse("text/plain; Charset=UTF-8; NAME=\"A B\""))
  test.eq(parsed.get("charset", "absent"), "UTF-8")?
  test.eq(parsed.get("name", "absent"), "A B")?
  test.eq(parsed.get("NAME", "absent"), "absent")?
}

proc test_mime_parse_splits_on_semicolons_before_interpreting_quotes() [error] {
  # The split happens first, so a semicolon inside a quoted value ends the
  # parameter and leaves an unterminated quote behind.
  test.eq(parsed_type(mime.parse("text/plain; name=\"a;b\"")), "rejected")?
  test.error_kind(mime.parse("text/plain; name=\"a;b\""), "mime-parse")?

  # The other consequence of splitting first: a value is quoted only when its
  # own part starts with a quote.
  test.eq(parsed_params(mime.parse("text/plain; a=1; b=2")).get("b", "absent"), "2")?
  test.eq(parsed_params(mime.parse("text/plain; a=\"q\"")).get("a", "absent"), "q")?

  # Empty parts between semicolons are skipped, and semicolons alone are not
  # parameters.
  test.eq(parsed_type(mime.parse("text/plain;")), "text/plain")?
  test.eq(parsed_type(mime.parse("text/plain;;")), "text/plain")?
  test.eq(parsed_params(mime.parse("text/plain;;")).get("a", "absent"), "absent")?
}

proc test_mime_parse_keeps_the_last_repeated_parameter() [error] {
  # A repeated name replaces the earlier value, and the names collide only
  # after ASCII case folding.
  test.eq(parsed_params(mime.parse("text/plain; a=1; a=2")).get("a", "absent"), "2")?
  test.eq(parsed_params(mime.parse("text/plain; a=1; A=2")).get("a", "absent"), "2")?
  test.eq(parsed_params(mime.parse("text/plain; a=1; b=2; a=3")).get("b", "absent"), "2")?
}

proc test_mime_parse_reads_quoted_parameter_values() [error] {
  # The outer quotes are removed; a backslash keeps the next character
  # literally, so an escaped quote or backslash survives as data.
  test.eq(parsed_params(mime.parse("text/plain; name=\"a b\"")).get("name", "absent"), "a b")?
  test.eq(parsed_params(mime.parse("text/plain; name=\"a\\\"b\"")).get("name", "absent"), "a\"b")?
  test.eq(parsed_params(mime.parse("text/plain; name=\"a\\\\b\"")).get("name", "absent"), "a\\b")?

  # An empty quoted value is a value; the parameter is not dropped.
  test.eq(parsed_params(mime.parse("text/plain; name=\"\"")).get("name", "absent"), "")?

  # Whitespace around the name and the value is trimmed before either is
  # interpreted, so a quoted value need not start at the `=`.
  test.eq(parsed_params(mime.parse("text/plain ;  charset = UTF-8 ")).get("charset", "absent"), "UTF-8")?
  test.eq(parsed_params(mime.parse("text/plain; name= \"a b\"")).get("name", "absent"), "a b")?
}

proc test_mime_parse_rejects_invalid_media_types() [error] {
  # The type field must be a non-empty `token/token` pair.
  for value in ["", "text", "/", "/plain", "not a media type", "*/*", "text/*", "text /plain"] {
    test.eq(parsed_type(mime.parse(value)), "rejected")?
  }
  test.error_kind(mime.parse(""), "mime-parse")?
}

proc test_mime_parse_rejects_malformed_parameters() [error] {
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
    test.eq(parsed_type(mime.parse(value)), "rejected")?
  }
  test.error_kind(mime.parse("text/plain; bad"), "mime-parse")?

  # One malformed parameter rejects the whole value rather than dropping the
  # parameter.
  test.eq(parsed_type(mime.parse("text/plain; a=1; bad; b=2")), "rejected")?
}
