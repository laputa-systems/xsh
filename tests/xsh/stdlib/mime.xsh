proc test_mime_lookup_and_parse() [fs, error] {
  let info = mime.lookup_ext("tar.gz") ?? {mime: "missing", exts: ["missing"]}
  test.eq(info.mime, "application/tar+gzip")?
  test.eq(info.exts[0], "tar.gz")?
  test.eq(mime.lookup_path(p"archive.tar.gz")?.mime, "application/tar+gzip")?
  test.eq(mime.lookup_ext("definitelymissingxsh"), null)?
  let parsed = mime.parse("Text/Plain; Charset=UTF-8")?
  test.eq(parsed.type, "text/plain")?
  test.eq(parsed.params.get("charset", ""), "UTF-8")?
  test.error_kind(mime.parse("not a media type"), "mime-parse")?
}
