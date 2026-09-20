##! Embedded implementation of the public `mime` module.
#
# A declared error variant reports `Family.Variant` unless its payload carries a
# string `kind` field, so `kind` is what keeps the baseline spelling
# `mime-lookup` visible to callers on the one entry whose runtime boundary
# reports an error rather than the declared optional.
error MimeLookupError = Miss(kind: Str, message: Str)
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# builtin extension data, the `/etc/mime.types` overlay interpretation, path
# suffix selection, the restricted media-type grammar, and result construction
# live here.
#
# The baseline has no persistent table: `lookup_ext` builds the builtin table
# and applies the host overlay once per call, and `lookup_path` calls
# `lookup_ext` once per candidate suffix. This port keeps exactly those
# invocation points, so `/etc/mime.types` is re-read at every lookup and
# nothing is cached for the life of the process. A read failure of any kind is
# ignored, exactly as the baseline ignores it.
#
# The baseline's flat `extension -> entry` map is reproduced without the map:
# an insert of a later row replaces an earlier row's value for the same key, so
# the surviving entry for a key is the last row that carries it. Selecting that
# row from the reverse of the overlay rows and then the reverse of the builtin
# rows is the same lookup with the same winner, and it costs one pass over
# small records instead of a copied map per extension.

# The one error kind `parse` reports.
#
# A declared error variant reports `Family.Variant` unless its payload carries
# a string `kind` field, so `kind` is what keeps the baseline spelling
# `mime-parse` visible to callers.
error MimeParseError = Malformed(kind: Str, message: Str)

# A media type after interpretation, or an invalid marker.
#
# The field is spelled `media` rather than `type` because `type` is a reserved
# word and cannot name a schema field; the public `MimeParse` record is
# constructed from it explicitly.
type ParsedMedia = {valid: Bool, media: Str, params: Map[Str]}

# One `name=value` parameter, or an invalid marker.
type ParsedParam = {valid: Bool, key: Str, value: Str}

# One parameter value, or an invalid marker.
type ParsedParamValue = {valid: Bool, value: Str}

# One `/etc/mime.types` line after interpretation, or an ignored marker.
type OverlayRow = {valid: Bool, mime: Str, exts: List[Str]}

# ASCII-only lowercase.
#
# The baseline normalizes with `to_ascii_lowercase`, which maps `A`-`Z` and
# leaves every other scalar, including every non-ASCII scalar, unchanged.
# `Str.lower` is Unicode case folding, so it is not a substitute here.
pure ascii_lower(text: Str) -> Str {
  return text.translate("ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz")
}

# Byte width of the UTF-8 character starting at `index`.
#
# Text is always valid UTF-8, so the leading byte decides. A byte that cannot
# start a character reports width one, keeping the scan moving forward.
pure character_width(text: Str, index: Int) -> Int {
  let first = text.byte_at(index, 0)
  if first < 194 {
    return 1
  }
  if first < 224 {
    return 2
  }
  if first < 240 {
    return 3
  }
  return 4
}

# Whether one byte may appear in an RFC 7230 token, which is the character
# class the baseline's `is_token` accepts as `ALPHA / DIGIT / "!" / "#" / "$" /
# "&" / "^" / "_" / "." / "+" / "-"`.
pure token_byte(byte: Int) -> Bool {
  if byte >= 97 and byte <= 122 {
    return true
  }
  if byte >= 65 and byte <= 90 {
    return true
  }
  if byte >= 48 and byte <= 57 {
    return true
  }
  return byte == 33
    or byte == 35
    or byte == 36
    or byte == 38
    or byte == 94
    or byte == 95
    or byte == 46
    or byte == 43
    or byte == 45
}

# Whether a whole string is a non-empty token.
#
# The test is byte-wise, and no byte of a non-ASCII character is an ASCII token
# byte, so a non-ASCII character always fails it.
pure is_token(value: Str) -> Bool {
  let length = value.byte_len()
  if length == 0 {
    return false
  }
  var index = 0
  while index < length {
    if !token_byte(value.byte_at(index, 0)) {
      return false
    }
    index = index + 1
  }
  return true
}

# Normalize one extension spelling: drop every leading dot, then lowercase.
#
# Leading dots are dropped before the case fold, and only ASCII case is folded,
# matching the baseline normalizer.
pure normalize_ext(ext: Str) -> Str {
  var start = 0
  while start < ext.byte_len() and ext.byte_at(start, 0) == 46 {
    start = start + 1
  }
  if start == 0 {
    return ascii_lower(ext)
  }
  return ascii_lower(ext.byte_slice(start))
}

# Normalize an extension list in order, dropping empty spellings.
#
# The dropping happens after normalization, so a spelling that is only dots
# disappears here. A row may therefore end up with no extensions at all; the
# baseline inserts no key for such a row but keeps the row itself.
pure normalize_exts(exts: List[Str]) -> List[Str] {
  let normalized = [normalize_ext(ext) for ext in exts]
  return [ext for ext in normalized if ext != ""]
}

# An invalid media type.
pure invalid_media() -> ParsedMedia {
  return {valid: false, media: "", params: map.empty()}
}

# An invalid parameter.
pure invalid_param() -> ParsedParam {
  return {valid: false, key: "", value: ""}
}

# An invalid parameter value.
pure invalid_param_value() -> ParsedParamValue {
  return {valid: false, value: ""}
}

# Interpret an unquoted parameter value, which must be a whole token.
pure unquoted_param_value(value: Str) -> ParsedParamValue {
  if is_token(value) {
    return {valid: true, value: value}
  }
  return invalid_param_value()
}

# Interpret a quoted parameter value.
#
# The value keeps everything between the outer quotes. `\` removes itself and
# keeps the next character literally, so an escaped quote is data and an
# unescaped inner quote rejects the whole value. A trailing `\` also rejects
# it, because the character it would escape is missing.
pure quoted_param_value(value: Str) -> ParsedParamValue {
  let inner = value.byte_slice(1)
  let length = inner.byte_len()
  if length == 0 or inner.byte_at(length - 1, 0) != 34 {
    return invalid_param_value()
  }
  let body = inner.byte_slice(0, length - 1)
  let body_length = body.byte_len()
  var output = ""
  var literal = 0
  var index = 0
  var escaped = false
  while index < body_length {
    if escaped {
      escaped = false
      index = index + character_width(body, index)
      continue
    }
    let byte = body.byte_at(index, 0)
    if byte == 92 {
      output = output + body.byte_slice(literal, index - literal)
      index = index + 1
      if index >= body_length {
        return invalid_param_value()
      }
      literal = index
      escaped = true
      continue
    }
    if byte == 34 {
      return invalid_param_value()
    }
    index = index + character_width(body, index)
  }
  return {valid: true, value: output + body.byte_slice(literal, body_length - literal)}
}

# Interpret one parameter value: a quoted string or a bare token.
#
# The baseline splits the media type on `;` and interprets each part on its
# own, so a value is quoted only when the part itself begins with a quote.
pure parse_param_value(value: Str) -> ParsedParamValue {
  if value.starts_with("\"") {
    return quoted_param_value(value)
  }
  return unquoted_param_value(value)
}

# Interpret one `name=value` parameter part.
#
# The name is trimmed and ASCII-lowercased before it is validated as a token,
# and the value is trimmed before it is interpreted.
pure parse_param(part: Str) -> ParsedParam {
  let equals = part.find("=")
  if equals < 0 {
    return invalid_param()
  }
  let name = ascii_lower(part.byte_slice(0, equals).trim())
  if !is_token(name) {
    return invalid_param()
  }
  let value = parse_param_value(part.byte_slice(equals + 1).trim())
  if !value.valid {
    return invalid_param()
  }
  return {valid: true, key: name, value: value.value}
}

# Interpret a media type into its normalized type and its parameters.
#
# Only the restricted baseline grammar is accepted. The value is split on the
# first `;`, the type must be `token/token`, and each remaining part must be a
# `name=value` parameter whose name is a token. Parts that are empty after
# trimming are skipped; a parameter that repeats keeps the last value, because
# the map is built once and a later entry replaces an earlier one. The returned
# type is the ASCII-lowercased `type/subtype`; parameter values keep their
# case.
pure parse_media_type(value: Str) -> ParsedMedia {
  let segments = value.split(";", 1)
  let head = segments.get(0, "").trim()
  let slash = head.find("/")
  if slash < 0 {
    return invalid_media()
  }
  let ty = head.byte_slice(0, slash)
  let subtype = head.byte_slice(slash + 1)
  if !is_token(ty) or !is_token(subtype) {
    return invalid_media()
  }
  let tail = segments.get(1, "").split(";", -1)
  let params = [parse_param(part.trim()) for part in tail if part.trim() != ""]
  if [param for param in params if !param.valid].len() > 0 {
    return invalid_media()
  }
  return {
    valid: true,
    media: ascii_lower(ty) + "/" + ascii_lower(subtype),
    params: {param.key: param.value for param in params},
  }
}

# The text before the first `#`, which is the whole line when it has none.
#
# No whitespace is trimmed here; the caller trims the result, exactly as the
# baseline trims after it splits the comment off.
pure strip_comment(line: Str) -> Str {
  let at = line.find("#")
  if at < 0 {
    return line
  }
  return line.byte_slice(0, at)
}

# The final path component, or the whole text when it has no `/`.
pure base_name(text: Str) -> Str {
  var end = text.byte_len()
  while end > 0 {
    if text.byte_at(end - 1, 0) == 47 {
      return text.byte_slice(end)
    }
    end = end - 1
  }
  return text
}

# Candidate extensions of a path spelling, longest suffix first.
#
# Only the final component is considered. Every suffix that starts right after
# a dot is a candidate, so `package.tar.gz` offers `tar.gz` before `gz`, and a
# name that is a single leading dot still offers the rest of the name. A name
# with no dot offers nothing. Each candidate is ASCII-lowercased, so the
# caller's normalization sees an already-folded suffix.
pure path_extensions(text: Str) -> List[Str] {
  let name = base_name(text)
  let starts = [
    index + 1
    for index in range(name.byte_len())
    if name.byte_at(index, 0) == 46
  ]
  return [ascii_lower(name.byte_slice(start)) for start in starts]
}

# Interpret one host overlay line.
#
# The line is cut at its first `#` and trimmed; an empty remainder is ignored.
# The first whitespace-separated field is the media type and the rest are
# extensions. A line is ignored when it has no extension field or when its
# media type does not satisfy the restricted grammar, which is the only
# validation the baseline applies to the field. The stored media type is the
# ASCII-lowercased field as written, not the normalized `type/subtype`, so a
# field that carries parameters keeps them.
pure overlay_row(line: Str) -> OverlayRow {
  let raw = strip_comment(line).trim()
  if raw == "" {
    return {valid: false, mime: "", exts: []}
  }
  let head = raw.words().get(0, "")
  let exts = raw.byte_slice(head.byte_len()).words()
  if exts.len() == 0 or !parse_media_type(head).valid {
    return {valid: false, mime: "", exts: []}
  }
  return {valid: true, mime: ascii_lower(head), exts: normalize_exts(exts)}
}

# Every accepted overlay row, in file order.
#
# Lines that the baseline ignores are dropped, so a returned list holds only
# rows that would have been inserted.
pure overlay_rows(text: Str) -> List[MimeInfo] {
  let parsed = [overlay_row(line) for line in text.lines()]
  return [{mime: row.mime, exts: row.exts} for row in parsed if row.valid]
}

# The builtin extension data, in table order.
#
# Each row carries the full extension list of its media type, so a lookup by
# any one of those extensions reports the whole list, exactly as the baseline
# shares one entry across the keys of a line.
pure builtin_rows() -> List[MimeInfo] {
  return [
    {mime: "application/gzip", exts: ["gz"]},
    {mime: "application/json", exts: ["json"]},
    {mime: "application/octet-stream", exts: ["bin"]},
    {mime: "application/pdf", exts: ["pdf"]},
    {mime: "application/tar+gzip", exts: ["tar.gz", "tgz"]},
    {mime: "application/x-tar", exts: ["tar"]},
    {mime: "application/xml", exts: ["xml"]},
    {mime: "application/zip", exts: ["zip"]},
    {mime: "image/gif", exts: ["gif"]},
    {mime: "image/jpeg", exts: ["jpg", "jpeg"]},
    {mime: "image/png", exts: ["png"]},
    {mime: "image/svg+xml", exts: ["svg"]},
    {mime: "text/css", exts: ["css"]},
    {mime: "text/csv", exts: ["csv"]},
    {mime: "text/html", exts: ["html", "htm"]},
    {mime: "text/markdown", exts: ["md", "markdown"]},
    {mime: "text/plain", exts: ["txt", "text", "log"]},
    {mime: "text/x-shellscript", exts: ["sh"]},
  ]
}

# The row that owns `key`, searching from the last inserted row backwards.
#
# Only a normalized non-empty key is matched, and a row that lost all of its
# extensions owns nothing. `Null` means the key is absent from these rows.
pure last_row(rows: List[MimeInfo], key: Str) -> MimeInfo? {
  var index = rows.len() - 1
  while index >= 0 {
    let row = rows[index]
    if row.exts.contains(key) {
      return row
    }
    index = index - 1
  }
  return null
}

# The host overlay text, or the empty string when it cannot be read.
#
# Every read failure is ignored, so a missing, unreadable, or non-UTF-8
# `/etc/mime.types` contributes no rows.
proc overlay_text() [fs] -> Str {
  return fs.read_text(p"/etc/mime.types") ?? ""
}

## Look up a MIME type by file extension.
##
## The extension is normalized first: every leading dot is dropped and ASCII
## case is folded, so `.TXT` and `txt` are the same key. An extension that
## normalizes to the empty string is not a lookup at all and yields `Null`.
##
## The host overlay is applied over the builtin data, so a host row replaces a
## builtin row for the same extension, and a later host row replaces an earlier
## one. `Null` means no row carries the extension.
export proc lookup_ext(ext: Str) [fs] -> MimeInfo? {
  let key = normalize_ext(ext)
  if key == "" {
    return null
  }
  let host = last_row(overlay_rows(overlay_text()), key)
  if host != null {
    return host
  }
  return last_row(builtin_rows(), key)
}

## Look up a MIME type from a path's extension.
##
## Only the final path component is inspected, and its compound suffixes are
## tried longest first, so `package.tar.gz` matches `tar.gz` before `gz`. The
## host overlay is consulted once per candidate suffix, in that order, and the
## first candidate that has an entry wins.
##
## The declared result is a plain optional, but the baseline runtime reports
## `Ok(entry)` on a hit and `Err(...)` on a miss for this one entry — a
## documented ABI adaptation, because the implementation must reproduce the
## observable runtime boundary rather than the declared type. A caller that
## propagates the result with `?` keeps working exactly as before.
export proc lookup_path(path: Path) [fs] -> Result[MimeInfo] {
  for ext in path_extensions(path.display()) {
    let hit = lookup_ext(ext)
    if hit != null {
      return Ok(hit)
    }
  }
  return Err(MimeLookupError.Miss(kind: "mime-lookup", message: "no MIME entry for path"))
}

## Parse a media type into structured fields.
##
## The accepted grammar is the baseline's restricted one: the value is split on
## semicolons before any quoted value is interpreted, so a `;` inside a quoted
## parameter value ends the parameter rather than being part of it. The
## `type/subtype` field must be a token pair, parameter names must be tokens,
## and a parameter value is either a token or a quoted string whose embedded
## quotes are escaped with `\`. The type field and the parameter names are
## ASCII-lowercased and the parameter values are kept as written; a repeated
## parameter keeps its last value.
##
## Malformed input returns `Err` of kind `mime-parse` rather than a partial
## record.
export pure parse(value: Str) -> Result[MimeParse] {
  let parsed = parse_media_type(value)
  if !parsed.valid {
    return Err(MimeParseError.Malformed(kind: "mime-parse", message: "malformed media type"))
  }
  return Ok({"type": parsed.media, params: parsed.params})
}
