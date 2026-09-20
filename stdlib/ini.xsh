##! Embedded implementation of the public `ini` module.
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# embedded implementation lives here: the encoder, and the text composition
# `ini.write` runs ahead of its retained file write.
#
# The decoder stays native. `crates/xsht/src/cli/files.rs` and
# `crates/xshi/src/interactive/config.rs` call `xsh::host::ini::decode` for
# tooling configuration before script execution, and the runtime's `ini.decode`
# and `ini.read` entries call the same decoder, so `src/modules/ini.rs` keeps
# `decode`, `parse_ini`, and everything those two touch.
#
# `validate_key` and `validate_section` are genuinely shared: they are native
# and retained, and the decoder's `parse_ini` is now their only caller. The
# encoder below restates the same key and section policy for the encode path
# rather than reaching a deleted native helper; the native `encode`,
# `write_key_value`, and the `RuntimeOp::IniEncode`/`IniWrite` arms were
# removed when the registry bound `ini.encode` and `ini.write` to this module.
#
# Shape of the encoder, and why it is written this way.
#
# A `Record` exposes only `keys()`, `has()`, and `get()`; there is no bulk
# entries accessor, and evaluating a `Record` or `Map` as a method receiver
# copies the whole container, so every `get` costs the size of the record it
# reads. The code below therefore makes exactly one `get` per top-level field
# and one `get` per section field, and reuses those reads: the walk that
# classifies and validates an entry also produces that entry's output text, so
# no field is read twice. Everything that accumulates runs in bulk (list and
# map comprehensions) and is joined once, so no `push`, `extend`, or `set`
# copies a growing container in a loop.
error IniEncodeError = Encode(kind: Str, message: Str)

# A rejected encode carrying the encoder's own kind.
pure encode_error(kind: Str, message: Str) -> IniEncodeError {
  return IniEncodeError.Encode(kind: kind, message: message)
}

# Lowercase ASCII letters only, matching the baseline `str::to_ascii_lowercase`.
#
# `Str.lower` is Unicode case folding and changes scalars outside ASCII (for
# example `İ`), so it cannot stand in for the baseline normalization. `from` and
# `to` have the same length, so `translate` rewrites each ASCII letter and
# leaves every other scalar, including every non-ASCII scalar, untouched.
pure ascii_lower(text: Str) -> Str {
  return text.translate("ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz")
}

# Whether a key or section name is invalid.
#
# The baseline rejects an empty name and any name containing NUL, newline, `[`,
# or `]`. The test is byte-wise, which matches the baseline character test: no
# byte of a multi-byte scalar is below 0x80, so only a real occurrence of one of
# those four ASCII characters can match.
pure invalid_name(text: Str) -> Bool {
  if text.byte_len() == 0 {
    return true
  }
  var index = 0
  while index < text.byte_len() {
    let byte = text.byte_at(index, 0)
    if byte == 0 or byte == 10 or byte == 91 or byte == 93 {
      return true
    }
    index = index + 1
  }
  return false
}

# Reject a section key after normalization. Global keys are checked with the
# same rule but are never normalized.
pure validate_key(key: Str) -> Result[Unit] {
  if invalid_name(key) {
    return Err(encode_error("ini-key", "invalid INI key"))
  }
  return Ok()
}

# Reject a section name, which the baseline keeps exactly as written.
pure validate_section_name(name: Str) -> Result[Unit] {
  if invalid_name(name) {
    return Err(encode_error("ini-section", "invalid INI section"))
  }
  return Ok()
}

# One line of a key/value block.
#
# Index zero carries the key; every later index is a two-space continuation
# line. `split` keeps a trailing empty part, so a value ending in a newline
# emits a final continuation line holding only the indent, exactly as the
# baseline `str::split('\n')` iteration does.
pure block_line(index: Int, key: Str, parts: List[Str]) -> Str {
  if index == 0 {
    return key + " = " + parts.get(0, "")
  }
  return "  " + parts.get(index, "")
}

# One key/value block: the key line, then one continuation line per further
# newline-separated part of the value.
pure key_block(key: Str, value: Str) -> Str {
  let parts = value.split("\n")
  return [block_line(i, key, parts) for i in range(0, parts.len())].join("\n")
}

# One section field, validated in the order the baseline checks it.
#
# The baseline checks that a field's value is a string before it validates the
# normalized spelling of that field's key, so both checks run here, in that
# order, as this field's element of the section comprehension: the first
# offending field of the first offending section is what a caller sees.
pure field_entry(section: Record, raw: Str) -> Result[Record] {
  match section.get(raw) ?? null {
    text is Str => {
      let key = ascii_lower(raw)
      validate_key(key)?
      return Ok({key: key, text: text})
    }
    _ => return Err(encode_error("ini-encode", "INI section values must be strings"))
  }
}

# One section's output text: its header line, then its key blocks.
#
# Fields are read once, in the section's key order, as (normalized key, text)
# pairs. The baseline inserts those pairs into a map keyed by the normalized
# key, so a later field overwrites an earlier one and the output is ordered by
# normalized key; a map comprehension has exactly those semantics, and `keys()`
# and `values()` walk it in the same key order. An empty section contributes its
# header alone.
pure section_text(name: Str, section: Record) -> Result[Str] {
  let fields = [field_entry(section, raw)? for raw in section.keys()]
  let winners: Map[Str] = {field.key: field.text for field in fields}
  let keys = winners.keys()
  let texts = winners.values()
  let lines = [key_block(keys.get(i, ""), texts.get(i, "")) for i in range(0, keys.len())]
  if lines.len() == 0 {
    return Ok("[" + name + "]")
  }
  return Ok("[" + name + "]\n" + lines.join("\n"))
}

# One top-level entry: its kind and the text it contributes.
#
# This is the baseline's collection pass, entry by entry, in the record's key
# order: a string is a global, a record is a section, and every other value —
# including a map — is rejected. Section fields are validated as their section
# is reached, so a section-field rejection outranks a rejection of any entry
# that sorts after it, exactly as in the baseline. Global keys are not checked
# here.
pure entry_part(name: Str, entry: Any) -> Result[Record] {
  match entry {
    section is Record => return Ok({kind: "section", text: section_text(name, section)?})
    text is Str => return Ok({kind: "global", text: key_block(name, text)})
    _ => {
      return Err(encode_error("ini-encode", "INI records may contain only global string keys or section records"))
    }
  }
}

## Encode a record as INI text.
##
## Top-level fields are either global strings or section records whose values
## are all strings; anything else is rejected. Globals are written first as
## `key = value` in the record's key order, then each section as its `[name]`
## header followed by its fields, with one blank line between the global block
## and the first section and between consecutive sections. Global keys keep
## their spelling while section keys are lowercased, and a later field replaces
## an earlier field that lowercases to the same key.
##
## A value's embedded newlines become two-space continuation lines, one per
## line, so a multiline value reads back as the same string. Every emitted line
## ends with `\n`, and an empty record encodes to the empty string.
export pure encode(value: Record) -> Result[Str] {
  # One `keys()` read and one `get` per field: the record is read once, and the
  # parts walk below both validates and formats what was read.
  let names = value.keys()
  let entries = [value.get(name) ?? null for name in names]

  # The collection pass: validation and formatting in one ordered walk, so the
  # first rejection is the baseline's first rejection and no field is read
  # twice.
  let parts = [entry_part(names.get(i, ""), entries.get(i, null))? for i in range(0, names.len())]

  # Global keys are validated with their own spelling, after every section
  # field. A global key is never normalized against a section key.
  let empty_part = {kind: "", text: ""}
  for index in range(0, names.len()) {
    if parts.get(index, empty_part).kind == "global" {
      validate_key(names.get(index, ""))?
    }
  }

  # Section names are validated in sorted order, after the global keys.
  let section_names = [names.get(i, "") for i in range(0, names.len()) if parts.get(i, empty_part).kind == "section"]
  for name in section_names {
    validate_section_name(name)?
  }

  let globals = [part.text for part in parts if part.kind == "global"]
  let sections = [part.text for part in parts if part.kind == "section"]
  let blocks = [if i > 0 { "\n" + sections.get(i, "") } else { sections.get(i, "") } for i in range(0, sections.len())]

  # The blank line between the global block and the first section appears only
  # when both sides emit something, and an empty record emits nothing at all.
  if globals.len() == 0 and blocks.len() == 0 {
    return Ok("")
  }
  if globals.len() > 0 and blocks.len() > 0 {
    return Ok(globals.join("\n") + "\n\n" + blocks.join("\n") + "\n")
  }
  if globals.len() > 0 {
    return Ok(globals.join("\n") + "\n")
  }
  return Ok(blocks.join("\n") + "\n")
}

## Write a record to a path as INI text.
##
## The text is encoded before the destination is examined, exactly as the
## baseline orders it, so an encoding failure is reported even when the
## destination exists and the write would have been refused. A `false`
## `overwrite` reports `ini-write` for an existing destination instead of
## replacing it. The file write itself stays the retained native operation.
export proc write(path: Path, value: Record, overwrite: Bool = true) [fs] -> Result[Unit] {
  var text = ""
  match encode(value) {
    Ok(encoded) => { text = encoded }
    Err(failure) => return Err(failure)
  }
  if !overwrite {
    var destination_exists = false
    match fs.exists(path) {
      Ok(present) => { destination_exists = present }
      Err(failure) => return Err(failure)
    }
    if destination_exists {
      return Err(
        IniWriteError.Destination(kind: "ini-write", message: "destination exists")
      )
    }
  }
  return fs.write(path, text)
}

# A refused write carrying the baseline's own kind.
error IniWriteError = Destination(kind: Str, message: Str)
