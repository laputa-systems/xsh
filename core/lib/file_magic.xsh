##! Deterministic content classification without a libmagic database.

type Identification = {description: Str, mime: Str, encoding: Str}

pure identified(description: Str, media_type: Str, encoding = "binary") -> Identification {
  {description: description, mime: media_type, encoding: encoding}
}

## Classify a bounded content sample by signatures and text encoding.
export pure classify(data: Bytes) -> Identification {
  if data.len() == 0 { return identified("empty", "inode/x-empty") }
  for magic in [
    {prefix: b"\x89PNG\r\n\x1a\n", description: "PNG image data", mime: "image/png"},
    {prefix: b"\xff\xd8\xff", description: "JPEG image data", mime: "image/jpeg"},
    {prefix: b"GIF87a", description: "GIF image data, version 87a", mime: "image/gif"},
    {prefix: b"GIF89a", description: "GIF image data, version 89a", mime: "image/gif"},
    {prefix: b"II*\0", description: "TIFF image data, little-endian", mime: "image/tiff"},
    {prefix: b"MM\0*", description: "TIFF image data, big-endian", mime: "image/tiff"},
    {prefix: b"%PDF-", description: "PDF document", mime: "application/pdf"},
    {prefix: b"PK\x03\x04", description: "Zip archive data", mime: "application/zip"},
    {prefix: b"PK\x05\x06", description: "Zip archive data (empty)", mime: "application/zip"},
    {prefix: b"\x1f\x8b", description: "gzip compressed data", mime: "application/gzip"},
    {prefix: b"BZh", description: "bzip2 compressed data", mime: "application/x-bzip2"},
    {prefix: b"\xfd7zXZ\0", description: "XZ compressed data", mime: "application/x-xz"},
    {prefix: b"\x28\xb5\x2f\xfd", description: "Zstandard compressed data", mime: "application/zstd"},
    {prefix: b"7z\xbc\xaf\x27\x1c", description: "7-zip archive data", mime: "application/x-7z-compressed"},
    {prefix: b"!<arch>\n", description: "current ar archive", mime: "application/x-archive"},
    {prefix: b"070701", description: "ASCII cpio archive (SVR4 with no CRC)", mime: "application/x-cpio"},
    {prefix: b"070702", description: "ASCII cpio archive (SVR4 with CRC)", mime: "application/x-cpio"},
    {prefix: b"SQLite format 3\0", description: "SQLite 3.x database", mime: "application/vnd.sqlite3"},
    {prefix: b"\0asm", description: "WebAssembly binary module", mime: "application/wasm"},
    {prefix: b"OggS", description: "Ogg data", mime: "application/ogg"},
    {prefix: b"fLaC", description: "FLAC audio bitstream data", mime: "audio/flac"},
  ] {
    if data.starts_with(magic.prefix) { return identified(magic.description, magic.mime) }
  }
  if data.len() >= 262 and data[257..262] == b"ustar" { return identified("POSIX tar archive", "application/x-tar") }
  if data.starts_with(b"RIFF") and data.len() >= 12 {
    if data[8..12] == b"WAVE" { return identified("RIFF WAVE audio", "audio/x-wav") }
    if data[8..12] == b"WEBP" { return identified("Web/P image", "image/webp") }
    if data[8..12] == b"AVI " { return identified("RIFF AVI video", "video/x-msvideo") }
  }
  if data.len() >= 12 and data[4..8] == b"ftyp" { return identified("ISO Media container", "video/mp4") }
  if data.starts_with(b"\xff\xfe") { return identified("Unicode text, UTF-16, little-endian", "text/plain", "utf-16le") }
  if data.starts_with(b"\xfe\xff") { return identified("Unicode text, UTF-16, big-endian", "text/plain", "utf-16be") }
  var ascii = true
  var control = false
  for at in range(data.len()) {
    let byte = data.byte_at(at) ?? 0
    if byte >= 128 { ascii = false }
    if byte == 0 or (byte < 32 and byte != 8 and byte != 9 and byte != 10 and byte != 12 and byte != 13 and byte != 27) { control = true }
  }
  if control { return identified("data", "application/octet-stream") }
  let text = data.utf8()
  if text is Err(_) { return identified("data", "application/octet-stream") }
  let contents = text ?? ""
  let encoding = if ascii { "us-ascii" } else { "utf-8" }
  let description = if ascii { "ASCII text" } else { "Unicode text, UTF-8 text" }
  if contents.starts_with("#!") {
    let first = contents.lines().get(0) ?? ""
    let interpreter = first.byte_slice(2).trim()
    return identified(f"{interpreter} script, {description} executable", "text/x-script", encoding)
  }
  if contents.trim().starts_with("<?xml") { return identified("XML document text", "text/xml", encoding) }
  let lower = contents.trim().lower()
  if lower.starts_with("<!doctype html") or lower.starts_with("<html") { return identified("HTML document text", "text/html", encoding) }
  if (lower.starts_with("{") or lower.starts_with("[")) and json.decode(contents) is Ok(_) { return identified("JSON text data", "application/json", encoding) }
  identified(description, "text/plain", encoding)
}
