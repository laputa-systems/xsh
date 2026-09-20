##! Embedded implementation of the public `cli` module.
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# argument policy lives here.
#
# Shape of the policy, and why it is written this way.
#
# A schema is a record of descriptors keyed by argument name. It is read
# through `Record.keys()` and `Record.get`, which visit names in sorted order,
# so schema iteration here matches the baseline's `BTreeMap` exactly. That
# order is load-bearing: it decides which duplicate, conflict, or reserved-name
# rejection is reported first, the order options are listed in usage, and the
# order positional fields consume operands.
#
# Presence is tracked as a set of names, never as a property of a value, so an
# option explicitly set to its default is still present for duplicate,
# conflict, and required checks, exactly as in the baseline.
#
# Every rejection carries the baseline spellings. A declared error variant
# reports `Family.Variant` unless its payload carries a string `kind` field, so
# `kind` is what keeps `cli-parse`, `cli-commands`, and `nul-path` visible to
# the outer CLI boundary, which decides stdout versus stderr and exit status
# from them.
#
# Entry points this module declares.
#
# Every public entry is implemented here: `cli.parse`, `cli.parse_full`,
# `cli.applet`, `cli.usage`, `cli.commands`, and `cli.tokens`.
#
# The `command` parameter a CLI entry labels its usage text with defaults to the
# name the program was invoked under (`self.command_name` in
# `src/runtime/eval/lowered_run.rs`, reached through the `command_name` bridge
# above). No source constant can spell that name, so each entry reads it at call
# time — the caller's argument when one was given, the bridge's value when none
# was — rather than binding it when the module is prepared.
#
# The baseline parameterizes the schema, descriptor, form, type, default,
# constraint, and usage machinery with a parse policy of its own (`ParsePolicy`
# in `src/modules/cli.rs`): strict for `parse`, `parse_full`, `usage`, and
# command walks, applet for `cli.applet`. Both are implemented here, selected by
# the policy spelling `"strict"` or `"applet"`, and they differ in exactly three
# places: `-h` is left to a descriptor that claims it, a conflicting option is
# reset instead of reported, and a repeated scalar overwrites instead of
# rejecting.
#
# The presentation of a rejection is the baseline's too: a help request is a
# `cli-help` rejection whose message is the rendered usage text, and a rejected
# value is a `cli-parse` rejection reported with the usage text appended and the
# `cli_usage` field set. What the outer CLI boundary makes of those — usage on
# stdout with status 0, a flagged rejection on stderr with status 2 — stays
# native, in `handle_cli_parse_stop`.

## Represent a record with one field set.
##
## Runtime representation bridge: lowering replaces every call with the private
## operation, so the body below is unreachable and raises if it is ever
## reached. It exists only to give the signature a shape that type-checks.
export pure record_with_field(record: Record, field: Str, value: Any) -> Record {
  return [record][record.keys().len()]
}

## The baseline's type name for a runtime value.
##
## Diagnostic bridge: lowering replaces every call with the private operation.
export pure type_name(value: Any) -> Str {
  return [value][1]
}

## The name this invocation was invoked as.
##
## Invocation-context bridge: lowering replaces every call with the private
## operation, which supplies the contextual command name the baseline defaults
## a CLI entry's `command` parameter to. The body below is unreachable and
## raises if it is ever reached.
export pure command_name() -> Str {
  return [""][1]
}

# The error family this module reports.
#
# A variant carries exactly the fields a rejection shows a caller: the kind the
# outer boundary dispatches on and the message the baseline spells. A baseline
# rejection has an empty payload, so the fields a caller can read are the kind
# and the message and nothing else is declared for `Reject`.
#
# `Report` is the same rejection after the entry-point policy has decided to show
# the usage text with it: the message is the rejection's own message followed by
# that text, and `cli_usage` is the field the outer CLI boundary tests for before
# it prints the message on stderr with status 2. A rejection the policy does not
# report that way keeps the `Reject` shape, so its payload gains nothing.
error CliError = Reject(kind: Str, message: Str) | Report(kind: Str, message: Str, cli_usage: Bool)

# A rejection raised while interpreting a schema, a descriptor, or a value.
pure cli_parse_error(message: Str) -> CliError {
  return CliError.Reject(kind: "cli-parse", message: message)
}

# A rejection raised while interpreting a command schema or dispatch.
pure cli_commands_error(message: Str) -> CliError {
  return CliError.Reject(kind: "cli-commands", message: message)
}

# The rejection a help request produces.
#
# Its message is the rendered usage text and its kind is `cli-help`, which the
# outer CLI boundary prints on stdout with status 0 rather than as a traceback.
pure cli_help_error(usage: Str) -> CliError {
  return CliError.Reject(kind: "cli-help", message: usage)
}

# The rejection an entry point reports: the failing message, the usage text, and
# the flag the outer CLI boundary dispatches on.
#
# The failing kind is carried through rather than replaced, so a rejection that
# did not come from the argument walk — a `nul-path` failure from a path value —
# is reported under its own kind and is not stopped at the boundary.
pure cli_usage_error(kind: Str, message: Str, usage: Str) -> CliError {
  return CliError.Report(
    kind: kind,
    message: message + "\n\n" + usage,
    cli_usage: true
  )
}

# A path whose text cannot be a path.
#
# The baseline reaches `PathValue::from_text` and reports its own `nul-path`
# error, so a NUL is rejected here before any cast and the kind is preserved
# rather than folded into `cli-parse`.
pure nul_path_error() -> CliError {
  return CliError.Reject(kind: "nul-path", message: "paths cannot contain NUL bytes")
}

# One token record, as the public `cli.tokens` shape describes it.
type CliToken = {kind: Str, name: Str, value: Str}

# A value type spelling, and whether the argument may repeat.
#
# The spelling is one of `Str`, `Int`, `UInt`, `Bool`, `Path`, or `Duration`,
# which is exactly the set `ArgValueType` covers, so a type is carried as its
# canonical name and compared by spelling.
type CliTypeName = {ty: Str, repeated: Bool}

# A form descriptor, as the `form:` (or `use:`) field spells it.
type CliForm = {
  raw: Str?,
  positional: Bool,
  repeated: Bool,
  optional_value: Bool,
  long: List[Str],
  short: List[Str],
}

# One interpreted option descriptor: the baseline's `OptionSpec`.
#
# Two fields adapt to what the language provides. An optional scalar is a
# nullable field rather than an absent one, and a default value is `Any` with
# `null` standing for "no default", which is unambiguous because a present
# default is always checked against the declared value type and `null` never
# matches one.
type CliOption = {
  name: Str,
  value_ty: Str,
  repeated: Bool,
  required: Bool,
  flag: Bool,
  positional: Bool,
  long: List[Str],
  short: List[Str],
  form: Str?,
  help: Str?,
  hidden: Bool,
  deprecated: Str?,
  optional_value: Bool,
  optional_default_value: Any,
  choices: List[Str],
  conflicts: List[Str],
  requires: List[Str],
  required_group: Str?,
  env: Str?,
  min: Int?,
  max: Int?,
  positive: Bool,
  nonzero: Bool,
  exists: Bool,
  file: Bool,
  dir: Bool,
  default_value: Any,
}

# A UTF-8 scalar and the width of its encoding.
type CliStep = {size: Int, scalar: Str}

# One interpreted command descriptor: the baseline's `CommandSpec`.
#
# `types` maps a positional name to its type spelling, with a name that is not
# listed taking `Str`, and `options` holds the interpreted specs of the
# command's own `options` descriptor. `rest` is the name the extra operands are
# collected under, when the command declares one.
type CliCommand = {
  canonical: Str,
  aliases: List[Str],
  positionals: List[Str],
  types: Record,
  options: List[CliOption],
  rest: Str?,
  min_rest: Int,
  command_like: Bool,
}

# A command schema's positional types, carried as a record.
type CliTypeMap = {types: Record}

# One command's arguments, split into the option walk's argv and its operands.
type CliSplit = {options: List[Str], positionals: List[Str]}

# A command form's positional names and rest name, as the form spells them.
type CliCommandForm = {positionals: List[Str], rest: Str?}

# One descriptor field read: whether it was present, and its value.
#
# These wrappers exist because of how the runtime coerces a function's returned
# value. A `Result` whose payload type is an `Optional` or `Any` is lowered with
# an `Any` payload, and the return coercion for `Result[Any]` tests the payload
# against `Any` *before* it tests for an error, so a returned `Err` reaches the
# caller as `Ok(<the error value>)`. A declared record payload keeps the error on
# the error channel, so a read that can fail and can be absent carries both facts
# in its payload instead of spelling them as `Result[Str?]`.
type CliText = {present: Bool, value: Str}

# One optional integer field read.
type CliCount = {present: Bool, value: Int}

# A converted value that can be of any type.
type CliValue = {value: Any}

# Whether `arg` is a negative number rather than a short option cluster.
#
# The baseline's test is a single leading `-` followed by an ASCII digit, so
# `-1` is an operand and `-x1` is a cluster.
pure looks_negative_number(arg: Str) -> Bool {
  if !arg.starts_with("-") {
    return false
  }
  let lead = arg.byte_at(1, 0)
  return lead >= 48 and lead <= 57
}

# One token record, built from the three fields the public shape declares.
pure arg_token(kind: Str, name: Str, value: Str) -> CliToken {
  let token: CliToken = {kind: kind, name: name, value: value}
  return token
}

# The UTF-8 scalar at `offset`, and the width of its encoding.
#
# `Str` is validated UTF-8, so the leading byte fixes the width and the scalar
# can be sliced without further checks. This mirrors the baseline's
# `char_indices` scan over a short-option cluster, which emits one token per
# scalar rather than one per byte.
pure utf8_step(text: Str, offset: Int) -> CliStep {
  let lead = text.byte_at(offset, 0)
  var size = 1
  if lead >= 240 {
    size = 4
  } else if lead >= 224 {
    size = 3
  } else if lead >= 192 {
    size = 2
  }
  let step: CliStep = {size: size, scalar: text.byte_slice(offset, size)}
  return step
}

## Split `argv` into long, short, and operand tokens.
##
## A token record has `kind` (`"long"`, `"short"`, or `"operand"`), `name` (the
## option name without its dashes, or the operand text itself), and `value`
## (the attached value, or the empty string when none is attached).
##
## `value_flags` names the long and short options that take a separate value:
## `--name value` and `-n value` become one token carrying the value, and
## `--name=value` and `-nvalue` attach it inline. Every other argument is an
## operand, including `-` alone, anything not starting with `-`, and a `-`
## followed by an ASCII digit, so a negative number is never read as a cluster.
##
## A `--` argument ends option scanning: it is not itself a token, and every
## later argument is an operand even if it looks like an option. A value flag
## whose value is missing is rejected rather than emitted with an empty value.
export pure tokens(
  argv: List[Str],
  value_flags: List[Str] = []
) -> Result[List[CliToken]] {
  let flags: Map[Bool] = {flag: true for flag in value_flags}
  # The token list is accumulated by `push` because one argument may consume
  # the next argument as its value and a short cluster emits one token per
  # scalar. The scan is bounded by the process command line, so the rebuilt
  # list stays proportional to `argv` rather than to program data.
  var collected: List[CliToken] = []
  var index = 0
  var operands_only = false
  while index < argv.len() {
    let arg = argv[index]
    if operands_only or arg == "-" or !arg.starts_with("-") or looks_negative_number(arg) {
      collected = collected.push(arg_token("operand", arg, ""))
      index = index + 1
      continue
    }
    if arg == "--" {
      operands_only = true
      index = index + 1
      continue
    }
    if arg.starts_with("--") {
      let parts = arg.byte_slice(2).split("=", 1)
      let name = parts.get(0, "")
      if name == "" {
        # An empty long name is not an option and not a value flag, so the
        # whole argument stays an operand.
        collected = collected.push(arg_token("operand", arg, ""))
        index = index + 1
        continue
      }
      var attached = ""
      if parts.len() == 2 {
        attached = parts.get(1, "")
      } else if flags.has(name) {
        let next = argv.get(index + 1, null)
        match next {
          text is Str => {
            attached = text
            index = index + 1
          }
          _ => return Err(cli_parse_error(f"option `--${name}` expects a value"))
        }
      }
      collected = collected.push(arg_token("long", name, attached))
      index = index + 1
      continue
    }
    let cluster = arg.byte_slice(1)
    var offset = 0
    while offset < cluster.byte_len() {
      let step = utf8_step(cluster, offset)
      if flags.has(step.scalar) {
        let after = offset + step.size
        if after < cluster.byte_len() {
          collected = collected.push(arg_token("short", step.scalar, cluster.byte_slice(after)))
        } else {
          let next = argv.get(index + 1, null)
          match next {
            text is Str => {
              collected = collected.push(arg_token("short", step.scalar, text))
              index = index + 1
            }
            _ => return Err(cli_parse_error(f"option `-${step.scalar}` expects a value"))
          }
        }
        break
      }
      collected = collected.push(arg_token("short", step.scalar, ""))
      offset = offset + step.size
    }
    index = index + 1
  }
  return Ok(collected)
}

# ASCII case folding to upper case, matching the baseline's
# `to_ascii_uppercase` for metavars and command positionals.
#
# `Str.upper` is Unicode case folding and rewrites scalars outside ASCII (for
# example `ÿ`), so it cannot stand in for the baseline normalization. `from`
# and `to` have the same length, so `translate` rewrites each ASCII letter and
# leaves every other scalar, including every non-ASCII scalar, untouched.
pure ascii_upper(word: Str) -> Str {
  return word.translate("abcdefghijklmnopqrstuvwxyz", "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
}

# Drop every leading `-`, matching the baseline's `trim_start_matches('-')`.
pure strip_leading_dashes(word: Str) -> Str {
  var index = 0
  while index < word.byte_len() and word.byte_at(index, 0) == 45 {
    index = index + 1
  }
  return word.byte_slice(index)
}

# The baseline's option-name normalization: leading dashes are dropped and
# inner dashes become underscores, so `--dry-run`, `dry-run`, and `dry_run` all
# name the same argument.
pure normalize_arg_name(name: Str) -> Str {
  return strip_leading_dashes(name).replace("-", "_")
}

# Whether a text carries a NUL byte.
#
# A NUL is the one text that cannot be a path, and the baseline rejects it in
# `PathValue::from_text` with its own `nul-path` kind rather than as a
# `cli-parse` rejection, so it is tested before any path is built. The test is
# byte-wise, as the baseline's is: no byte of a multi-byte scalar is zero.
pure has_nul(word: Str) -> Bool {
  var index = 0
  while index < word.byte_len() {
    if word.byte_at(index, 0) == 0 {
      return true
    }
    index = index + 1
  }
  return false
}

# A path from text, rejecting a NUL the way the baseline does.
pure path_from_text(word: Str) -> Result[Path] {
  if has_nul(word) {
    return Err(nul_path_error())
  }
  return Ok(Path(word))
}

# The baseline's `DurationValue::from_literal`: an unsigned decimal amount with
# a required `ms`, `s`, `m`, or `h` suffix, multiplied out with a checked
# multiplication. A bare number and a negative amount are not literals.
#
# The baseline accumulates in `u64`, so its range reaches `u64::MAX` millis. A
# `Duration` here is built with `time.millis`, which takes an `Int`, so an
# amount above `i64::MAX` cannot be represented and reads as unparseable; see
# the module notes. Everything below that limit matches the baseline exactly,
# including the leading `+` its integer parse accepts and the truncating digit
# range it rejects.
pure duration_from_literal(word: Str) -> Duration? {
  var amount = ""
  var multiplier = 0
  if word.ends_with("ms") {
    amount = word.byte_slice(0, word.byte_len() - 2)
    multiplier = 1
  } else if word.ends_with("s") {
    amount = word.byte_slice(0, word.byte_len() - 1)
    multiplier = 1000
  } else if word.ends_with("m") {
    amount = word.byte_slice(0, word.byte_len() - 1)
    multiplier = 60000
  } else if word.ends_with("h") {
    amount = word.byte_slice(0, word.byte_len() - 1)
    multiplier = 3600000
  } else {
    return null
  }
  var digits = amount
  if digits.starts_with("+") {
    digits = digits.byte_slice(1)
  }
  if digits == "" {
    return null
  }
  var total = 0
  var index = 0
  while index < digits.byte_len() {
    let byte = digits.byte_at(index, 0)
    if byte < 48 or byte > 57 {
      return null
    }
    # `i64::MAX / 10` and `i64::MAX - digit` are the two overflow tests for the
    # decimal accumulation, so a checked multiply and add are always safe here.
    if total > 922337203685477580 {
      return null
    }
    total = total * 10
    let digit = byte - 48
    if total > 9223372036854775807 - digit {
      return null
    }
    total = total + digit
    index = index + 1
  }
  if total > 9223372036854775807 / multiplier {
    return null
  }
  return time.millis(total * multiplier)
}

# A `Bool` descriptor field.
#
# An absent field leaves `fallback` in place; any other value is rejected, so a
# field present with a `null` is a rejection rather than an absent field. The
# `lead` word is `option` or `command`, which is the only difference between
# the baseline's two readers of this shape.
pure field_bool(fields: Record, lead: Str, owner: Str, field: Str, fallback: Bool) -> Result[Bool] {
  if !fields.has(field) {
    return Ok(fallback)
  }
  let raw = fields.get(field) ?? null
  match raw {
    flag is Bool => return Ok(flag)
    _ => return Err(cli_parse_error(
      f"${lead} `${owner}` descriptor field `${field}` must be Bool, found ${type_name(raw)}"
    ))
  }
}

# A `Str` descriptor field, absent when `present` is false.
pure field_string(fields: Record, lead: Str, owner: Str, field: Str) -> Result[CliText] {
  if !fields.has(field) {
    let absent: CliText = {present: false, value: ""}
    return Ok(absent)
  }
  let raw = fields.get(field) ?? null
  match raw {
    word is Str => {
      let found: CliText = {present: true, value: word}
      return Ok(found)
    }
    _ => return Err(cli_parse_error(
      f"${lead} `${owner}` descriptor field `${field}` must be Str, found ${type_name(raw)}"
    ))
  }
}

# An `Int` descriptor field, absent when `present` is false.
pure field_int(fields: Record, option: Str, field: Str) -> Result[CliCount] {
  if !fields.has(field) {
    let absent: CliCount = {present: false, value: 0}
    return Ok(absent)
  }
  let raw = fields.get(field) ?? null
  match raw {
    count is Int => {
      let found: CliCount = {present: true, value: count}
      return Ok(found)
    }
    _ => return Err(cli_parse_error(
      f"option `${option}` descriptor field `${field}` must be Int, found ${type_name(raw)}"
    ))
  }
}

# The `deprecated` field: `true` becomes the baseline's own message, a string is
# used as written, `false` is silence, and any other present value is rejected.
pure field_deprecated(fields: Record, option: Str) -> Result[CliText] {
  if !fields.has("deprecated") {
    let absent: CliText = {present: false, value: ""}
    return Ok(absent)
  }
  let raw = fields.get("deprecated") ?? null
  match raw {
    flag is Bool => {
      if flag {
        let noted: CliText = {present: true, value: f"option `${option}` is deprecated"}
        return Ok(noted)
      }
      let silent: CliText = {present: false, value: ""}
      return Ok(silent)
    }
    word is Str => {
      let found: CliText = {present: true, value: word}
      return Ok(found)
    }
    _ => return Err(cli_parse_error(
      f"option `${option}` descriptor field `deprecated` must be Bool or Str, found ${type_name(raw)}"
    ))
  }
}

# The optional scalar a `CliText` carries, or null when the field was absent.
pure optional_text(read: CliText) -> Str? {
  if read.present {
    return read.value
  }
  return null
}

# The optional integer a `CliCount` carries, or null when the field was absent.
pure optional_count(read: CliCount) -> Int? {
  if read.present {
    return read.value
  }
  return null
}

# One element of a name list: a `Str`, normalized as an option name.
#
# A list element that is not a string reports the baseline's list message, with
# no `found` clause, which is what separates it from a rejected field value.
pure name_item(option: Str, field: Str, item: Any) -> Result[Str] {
  match item {
    word is Str => return Ok(normalize_arg_name(word))
    _ => return Err(cli_parse_error(
      f"option `${option}` descriptor field `${field}` must be Str or List[Str]"
    ))
  }
}

# One element of a text list: a `Str`, kept as written.
pure text_item(option: Str, field: Str, item: Any) -> Result[Str] {
  match item {
    word is Str => return Ok(word)
    _ => return Err(cli_parse_error(
      f"option `${option}` descriptor field `${field}` must be Str or List[Str]"
    ))
  }
}

# The `long` and `short` fields: a single `Str` or a `List[Str]`, normalized as
# option names. An absent field is the empty list.
pure field_arg_names(fields: Record, option: Str, field: Str) -> Result[List[Str]] {
  if !fields.has(field) {
    return Ok([])
  }
  let raw = fields.get(field) ?? null
  match raw {
    word is Str => return Ok([normalize_arg_name(word)])
    items is List[Any] => {
      let names = [name_item(option, field, item)? for item in items]
      return Ok(names)
    }
    _ => return Err(cli_parse_error(
      f"option `${option}` descriptor field `${field}` must be Str or List[Str], found ${type_name(raw)}"
    ))
  }
}

# The `choices`, `conflicts`, and `requires` fields: a single `Str` or a
# `List[Str]`, kept as written. An absent field is the empty list.
pure field_strings(fields: Record, option: Str, field: Str) -> Result[List[Str]] {
  if !fields.has(field) {
    return Ok([])
  }
  let raw = fields.get(field) ?? null
  match raw {
    word is Str => return Ok([word])
    items is List[Any] => {
      let values = [text_item(option, field, item)? for item in items]
      return Ok(values)
    }
    _ => return Err(cli_parse_error(
      f"option `${option}` descriptor field `${field}` must be Str or List[Str], found ${type_name(raw)}"
    ))
  }
}

# A form's option names followed by the descriptor's own, in that order and
# without repeats, so a name spelled in both places is listed once.
pure merge_arg_names(first: List[Str], second: List[Str]) -> List[Str] {
  return first.extend([name for name in second if !first.contains(name)])
}

# The canonical spelling of a scalar value type, or null when the name is not
# one of the six the baseline knows.
pure parse_scalar(word: Str) -> Str? {
  let trimmed = word.trim()
  if trimmed == "Str" or trimmed == "Int" or trimmed == "UInt" or trimmed == "Bool" or trimmed == "Path" or trimmed == "Duration" {
    return trimmed
  }
  return null
}

# Interpret a declared type name: a scalar spelling, or one `List[...]` wrapper.
#
# A `List` of a `List` is rejected rather than flattened, and the wrapper is
# taken only when both the prefix and the closing bracket are present, so
# `List[Str` is an unsupported type name rather than a repeated `Str`.
pure parse_type_name(word: Str) -> Result[CliTypeName] {
  let trimmed = word.trim()
  if trimmed.starts_with("List[") and trimmed.ends_with("]") {
    let inner = trimmed.byte_slice(5, trimmed.byte_len() - 6)
    let parsed = parse_type_name(inner)?
    if parsed.repeated {
      return Err(cli_parse_error("nested List option types are not supported"))
    }
    let wrapped: CliTypeName = {ty: parsed.ty, repeated: true}
    return Ok(wrapped)
  }
  let scalar = parse_scalar(trimmed)
  if scalar == null {
    return Err(cli_parse_error(f"unsupported option type `${trimmed}`"))
  }
  let plain: CliTypeName = {ty: scalar ?? "Str", repeated: false}
  return Ok(plain)
}

# The value type a literal implies, or null when it has no scalar spelling.
pure infer_value_type(value: Any) -> Str? {
  match value {
    word is Str => return "Str"
    count is Int => return "Int"
    flag is Bool => return "Bool"
    target is Path => return "Path"
    span is Duration => return "Duration"
    _ => return null
  }
}

# The value type implied by a descriptor's default, when no explicit type is
# declared. A separate `present` flag keeps a `null` default distinct from an
# absent one, which is a rejection rather than a fallback.
#
# A list default makes the option repeated and takes its type from the first
# element; an empty list is a repeated `Str`. Every further element must have
# the same scalar type, and elements are examined in order, so the first
# offending element decides which of the two list messages is reported.
pure infer_descriptor_type(name: Str, raw: Any, present: Bool) -> Result[CliTypeName] {
  if !present {
    let absent: CliTypeName = {ty: "Str", repeated: false}
    return Ok(absent)
  }
  let direct = infer_value_type(raw)
  if direct != null {
    let single: CliTypeName = {ty: direct ?? "Str", repeated: false}
    return Ok(single)
  }
  match raw {
    items is List[Any] => {
      if items.len() == 0 {
        let empty: CliTypeName = {ty: "Str", repeated: true}
        return Ok(empty)
      }
      let first_ty = infer_value_type(items.get(0, null))
      if first_ty == null {
        return Err(cli_parse_error(f"option `${name}` default list contains unsupported values"))
      }
      var index = 1
      while index < items.len() {
        let item_ty = infer_value_type(items.get(index, null))
        if item_ty == null {
          return Err(cli_parse_error(f"option `${name}` default list contains unsupported values"))
        }
        if item_ty != first_ty {
          return Err(cli_parse_error(f"option `${name}` default list must contain one scalar type"))
        }
        index = index + 1
      }
      let repeated: CliTypeName = {ty: first_ty ?? "Str", repeated: true}
      return Ok(repeated)
    }
    _ => return Err(cli_parse_error(
      f"option `${name}` default cannot infer type from ${type_name(raw)}"
    ))
  }
}

# Convert one default value to the declared type, or reject it.
#
# Only `Path` and `Duration` accept a text spelling; an `Int`, `UInt`, or
# `Bool` default must already carry that type, so `{count: {kind: "Int",
# default: "3"}}` is rejected rather than parsed. `UInt` is a non-negative
# `Int`, and its own message names the type instead of reporting what was
# found.
pure convert_default_value(name: Str, value: Any, ty: Str) -> Result[CliValue] {
  match value {
    word is Str => {
      if ty == "Str" {
        return Ok({value: word})
      }
      if ty == "Path" {
        return Ok({value: path_from_text(word)?})
      }
      if ty == "Duration" {
        let span = duration_from_literal(word)
        if span == null {
          return Err(cli_parse_error(f"option `${name}` default does not parse as Duration"))
        }
        return Ok({value: span})
      }
      return Err(cli_parse_error(
        f"option `${name}` default does not match declared type, found Str"
      ))
    }
    count is Int => {
      if ty == "Int" {
        return Ok({value: count})
      }
      if ty == "UInt" {
        if count < 0 {
          return Err(cli_parse_error(
            f"option `${name}` default does not match declared type UInt"
          ))
        }
        return Ok({value: count})
      }
      return Err(cli_parse_error(
        f"option `${name}` default does not match declared type, found Int"
      ))
    }
    flag is Bool => {
      if ty == "Bool" {
        return Ok({value: flag})
      }
      return Err(cli_parse_error(
        f"option `${name}` default does not match declared type, found Bool"
      ))
    }
    target is Path => {
      if ty == "Path" {
        return Ok({value: target})
      }
      return Err(cli_parse_error(
        f"option `${name}` default does not match declared type, found Path"
      ))
    }
    span is Duration => {
      if ty == "Duration" {
        return Ok({value: span})
      }
      return Err(cli_parse_error(
        f"option `${name}` default does not match declared type, found Duration"
      ))
    }
    _ => return Err(cli_parse_error(
      f"option `${name}` default does not match declared type, found ${type_name(value)}"
    ))
  }
}

# The value a converted default carries.
pure unwrapped(converted: CliValue) -> Any {
  return converted.value
}

# Normalize a descriptor's default against the declared type and repetition.
#
# A repeated option's default must be a list, and every element is converted; a
# non-repeated option's default is converted as a single value. An absent
# default is carried as a `null` value, which is how the baseline tells "no
# default" from a default that converts to `null` — the latter cannot occur,
# because every declared value type is checked against the default.
pure normalize_default(
  name: Str,
  raw: Any,
  present: Bool,
  ty: Str,
  repeated: Bool
) -> Result[CliValue] {
  if !present {
    return Ok({value: null})
  }
  if repeated {
    match raw {
      items is List[Any] => {
        let converted = [
          unwrapped(convert_default_value(name, items.get(i, null), ty)?)
          for i in range(0, items.len())
        ]
        return Ok({value: converted})
      }
      _ => return Err(cli_parse_error(
        f"option `${name}` default must be List for repeated options"
      ))
    }
  }
  return convert_default_value(name, raw, ty)
}

# The explicit `kind` field, or its `type` alias, or null when neither is
# present.
#
# A field that is present must be a `Str`, and its rejection names `kind` even
# when the descriptor spelled it `type`, which is the baseline's message. An
# empty string is not an absent field: it means "infer from the default", as it
# does in the baseline.
pure explicit_type(fields: Record, name: Str) -> Result[CliText] {
  var field = ""
  if fields.has("kind") {
    field = "kind"
  } else if fields.has("type") {
    field = "type"
  } else {
    let absent: CliText = {present: false, value: ""}
    return Ok(absent)
  }
  let raw = fields.get(field) ?? null
  match raw {
    word is Str => {
      let found: CliText = {present: true, value: word}
      return Ok(found)
    }
    _ => return Err(cli_parse_error(
      f"option `${name}` kind must be Str, found ${type_name(raw)}"
    ))
  }
}

# Interpret a `form:` (or `use:`) text into option names, a positional marker,
# and the repetition and optional-value hints it spells.
#
# Tokens are read left to right. An option token contributes its names: a
# `--long` drops one trailing `]` and then splits at `[=` when both are
# present, which marks an optional value, and otherwise splits at `=`; a
# `-abc` contributes one short name per scalar. A token that is not an option
# is a positional only while no option token has been seen, and it marks the
# argument repeated when it starts with `...`.
#
# Each of the two name lists is rebuilt per token. A form is a declaration
# literal with a handful of tokens, so that cost is bounded by the declaration
# rather than by program data.
pure parse_form_descriptor(name: Str, fields: Record) -> Result[CliForm] {
  var raw: Any = null
  if fields.has("form") {
    raw = fields.get("form") ?? null
  } else if fields.has("use") {
    raw = fields.get("use") ?? null
  } else {
    let empty: CliForm = {
      raw: null,
      positional: false,
      repeated: false,
      optional_value: false,
      long: [],
      short: [],
    }
    return Ok(empty)
  }
  match raw {
    word is Str => {
      var long: List[Str] = []
      var short: List[Str] = []
      var positional = false
      var repeated = false
      var optional_value = false
      var has_option = false
      for token in word.words() {
        if token.starts_with("--") {
          let body = token.byte_slice(2)
          var option_name = body
          var takes_value = false
          if body.ends_with("]") {
            let undecorated = body.byte_slice(0, body.byte_len() - 1)
            let parts = undecorated.split("[=", 1)
            if parts.len() == 2 {
              option_name = parts.get(0, "")
              takes_value = true
            }
          }
          if !takes_value {
            option_name = body.split("=", 1).get(0, "")
          }
          if option_name == "" {
            return Err(cli_parse_error("empty long option in cli form"))
          }
          long = long.push(normalize_arg_name(option_name))
          optional_value = optional_value or takes_value
          has_option = true
        } else if token.starts_with("-") {
          let cluster = token.byte_slice(1)
          if cluster == "" {
            return Err(cli_parse_error("empty short option in cli form"))
          }
          var offset = 0
          while offset < cluster.byte_len() {
            let step = utf8_step(cluster, offset)
            short = short.push(step.scalar)
            offset = offset + step.size
          }
          has_option = true
        } else if !has_option {
          positional = true
          repeated = token.starts_with("...")
        }
      }
      let parsed: CliForm = {
        raw: word,
        positional: positional,
        repeated: repeated,
        optional_value: optional_value,
        long: long,
        short: short,
      }
      return Ok(parsed)
    }
    _ => return Err(cli_parse_error(
      f"option `${name}` descriptor field `form` must be Str, found ${type_name(raw)}"
    ))
  }
}

# Interpret one option descriptor: the baseline's `parse_descriptor`.
#
# The order of the steps carries meaning. The form is read first, so a bad form
# outranks a bad type; the explicit type is read next; the default supplies the
# type when none is declared and is then normalized against it; the flags that
# constrain repetition, position and value are read next, because they feed the
# `flag` and `required` defaults; and only then is every remaining field read.
# A caller therefore sees the baseline's first rejection, in the baseline's
# order, down to which field it names.
pure parse_option(name: Str, descriptor: Any) -> Result[CliOption] {
  match descriptor {
    word is Str => {
      let parsed = parse_type_name(word)?
      let spec: CliOption = {
        name: name,
        value_ty: parsed.ty,
        repeated: parsed.repeated,
        required: false,
        flag: parsed.ty == "Bool",
        positional: false,
        long: [],
        short: [],
        form: null,
        help: null,
        hidden: false,
        deprecated: null,
        optional_value: false,
        optional_default_value: null,
        choices: [],
        conflicts: [],
        requires: [],
        required_group: null,
        env: null,
        min: null,
        max: null,
        positive: false,
        nonzero: false,
        exists: false,
        file: false,
        dir: false,
        default_value: null,
      }
      return Ok(spec)
    }
    fields is Record => {
      let form = parse_form_descriptor(name, fields)?
      let declared = explicit_type(fields, name)?
      var parsed: CliTypeName = {ty: "Str", repeated: false}
      if !declared.present or declared.value == "" {
        parsed = infer_descriptor_type(name, fields.get("default") ?? null, fields.has("default"))?
      } else {
        parsed = parse_type_name(declared.value)?
      }
      let declared_repeated = field_bool(fields, "option", name, "repeated", false)?
      let repeated = declared_repeated or parsed.repeated or form.repeated
      let declared_positional = field_bool(fields, "option", name, "positional", false)?
      let positional = declared_positional or form.positional
      let flag_default = parsed.ty == "Bool" and !repeated and !positional
      let flag = field_bool(fields, "option", name, "flag", flag_default)?
      let converted_default = normalize_default(
        name, fields.get("default") ?? null, fields.has("default"), parsed.ty, repeated
      )?
      let default_value = converted_default.value
      # An explicit `required` always wins. Otherwise a non-repeated positional
      # is required only when no default supplies its absent value, so a form
      # with a default reads as an optional positional.
      var required = false
      if fields.has("required") {
        required = field_bool(fields, "option", name, "required", false)?
      } else {
        required = positional and !repeated and default_value == null
      }
      let long = merge_arg_names(form.long, field_arg_names(fields, name, "long")?)
      let short = merge_arg_names(form.short, field_arg_names(fields, name, "short")?)
      let help = optional_text(field_string(fields, "option", name, "help")?)
      let hidden = field_bool(fields, "option", name, "hidden", false)?
      let deprecated = optional_text(field_deprecated(fields, name)?)
      let optional_value = field_bool(fields, "option", name, "optional_value", form.optional_value)?
      let choices = field_strings(fields, name, "choices")?
      let conflicts = field_strings(fields, name, "conflicts")?
      let requires = field_strings(fields, name, "requires")?
      let required_group = optional_text(field_string(fields, "option", name, "required_group")?)
      let env_source = optional_text(field_string(fields, "option", name, "env")?)
      let min = optional_count(field_int(fields, name, "min")?)
      let max = optional_count(field_int(fields, name, "max")?)
      let positive = field_bool(fields, "option", name, "positive", false)?
      let nonzero = field_bool(fields, "option", name, "nonzero", false)?
      let exists = field_bool(fields, "option", name, "exists", false)?
      let file = field_bool(fields, "option", name, "file", false)?
      let dir = field_bool(fields, "option", name, "dir", false)?
      let converted_optional = normalize_default(
        name, fields.get("optional_default") ?? null, fields.has("optional_default"), parsed.ty, false
      )?
      let optional_default = converted_optional.value
      let spec: CliOption = {
        name: name,
        value_ty: parsed.ty,
        repeated: repeated,
        required: required,
        flag: flag,
        positional: positional,
        long: long,
        short: short,
        form: form.raw,
        help: help,
        hidden: hidden,
        deprecated: deprecated,
        optional_value: optional_value,
        optional_default_value: optional_default,
        choices: choices,
        conflicts: conflicts,
        requires: requires,
        required_group: required_group,
        env: env_source,
        min: min,
        max: max,
        positive: positive,
        nonzero: nonzero,
        exists: exists,
        file: file,
        dir: dir,
        default_value: default_value,
      }
      return Ok(spec)
    }
    _ => return Err(cli_parse_error(
      f"option `${name}` descriptor must be Str or Record, found ${type_name(descriptor)}"
    ))
  }
}

# Interpret one schema entry: its descriptor, then the reserved-help rule.
#
# `--help` is reserved under either policy. The short spelling is reserved only
# under the strict policy: an applet leaves `-h` to a descriptor that claims it,
# and reaching `-h` as help is then what the help scan decides. Both rejections
# are schema rejections, reported without usage text.
pure option_spec(name: Str, descriptor: Any, policy: Str) -> Result[CliOption] {
  let spec = parse_option(name, descriptor)?
  if normalize_arg_name(name) == "help" or spec.long.contains("help") {
    return Err(cli_parse_error("`--help` is reserved by cli.parse"))
  }
  if policy == "strict" and spec.short.contains("h") {
    return Err(cli_parse_error("`-h` is reserved by cli.parse"))
  }
  return Ok(spec)
}

# The names of a record, in the order the baseline reads them.
#
# A record here keeps the order its fields were inserted in, while the
# baseline's records are backed by an ordered map, so a schema that was built at
# run time rather than written as a literal would be walked in a different
# order — and that order decides which argument a schema error names first, what
# order the usage text lists options in, and which missing or conflicting
# argument is reported. Sorting the names makes the walk agree with the baseline
# whatever built the schema.
#
# The order is byte order, which is the baseline's string order. A schema is a
# handful of names, so the repeated minimum search costs nothing worth avoiding.
pure sorted_names(names: List[Str]) -> List[Str] {
  var remaining = names
  var ordered: List[Str] = []
  while remaining.len() > 0 {
    var smallest = remaining.get(0, "")
    for name in remaining {
      if name < smallest {
        smallest = name
      }
    }
    ordered = ordered.push(smallest)
    remaining = [name for name in remaining if name != smallest]
  }
  return ordered
}

# A record rebuilt with its fields in the order the baseline reads them back.
#
# A record here reads back in the order its fields were added, while the
# baseline's records read back in sorted order, so a result record a caller
# iterates is rebuilt in that order to keep the two the same.
pure sorted_record(fields: Record) -> Record {
  var ordered: Record = {}
  for name in sorted_names(fields.keys()) {
    ordered = record_with_field(ordered, name, fields.get(name) ?? null)
  }
  return ordered
}

# Interpret a whole schema in sorted name order, so the first rejection is the
# baseline's first rejection.
#
# The schema is read once, into parallel name and value lists, and each entry
# is then interpreted by index. A schema is a declaration literal, and one
# `Record.get` per field is what reading a record costs here.
pure schema_specs(schema: Record, policy: Str) -> Result[List[CliOption]] {
  let names = sorted_names(schema.keys())
  let raws = [schema.get(name) ?? null for name in names]
  return Ok([
    option_spec(names.get(i, ""), raws.get(i, null), policy)?
      for i in range(0, names.len())
  ])
}

# The spec a lookup that has already decided it will succeed stands in for.
#
# A placeholder is only read where its lookup has already established the
# answer, so it is never dispatched or rendered: it exists to keep the type of
# the looked-up spec known. It takes the shape of a hidden option with no names,
# so a caller that did render it would contribute nothing.
pure unrendered_spec() -> CliOption {
  let spec: CliOption = {
    name: "",
    value_ty: "Str",
    repeated: false,
    required: false,
    flag: false,
    positional: false,
    long: [],
    short: [],
    form: null,
    help: null,
    hidden: true,
    deprecated: null,
    optional_value: false,
    optional_default_value: null,
    choices: [],
    conflicts: [],
    requires: [],
    required_group: null,
    env: null,
    min: null,
    max: null,
    positive: false,
    nonzero: false,
    exists: false,
    file: false,
    dir: false,
    default_value: null,
  }
  return spec
}

# The label a positional contributes to the command line: its form exactly as
# written, or its name in upper case.
pure positional_label(spec: CliOption) -> Str {
  if spec.form != null {
    return spec.form ?? ""
  }
  return ascii_upper(spec.name)
}

# Drop every leading `...`, matching the baseline's `trim_start_matches("...")`.
pure trim_leading_ellipsis(word: Str) -> Str {
  var rest = word
  while rest.starts_with("...") {
    rest = rest.byte_slice(3)
  }
  return rest
}

# Drop every trailing `]`, matching the baseline's `trim_end_matches(']')`.
pure trim_trailing_brackets(word: Str) -> Str {
  var end = word.byte_len()
  while end > 0 and word.byte_at(end - 1, 0) == 93 {
    end = end - 1
  }
  return word.byte_slice(0, end)
}

# The placeholder an option's value is introduced by.
#
# The form is read backwards for its last non-option token, which gives the
# metavar; an option token contributes the text after `[=` (with its trailing
# bracket dropped) or after `=`. A form with no such token leaves the option's
# own name in upper case.
pure usage_metavar(spec: CliOption) -> Str {
  if spec.form != null {
    let tokens = (spec.form ?? "").words()
    var index = tokens.len() - 1
    while index >= 0 {
      let token = tokens.get(index, "")
      if !token.starts_with("-") {
        return trim_leading_ellipsis(token)
      }
      let optional_parts = token.split("[=", 1)
      if optional_parts.len() == 2 {
        return trim_trailing_brackets(optional_parts.get(1, ""))
      }
      let attached = token.split("=", 1)
      if attached.len() == 2 {
        return attached.get(1, "")
      }
      index = index - 1
    }
  }
  return ascii_upper(spec.name)
}

# The names an option is introduced by, in the order the baseline lists them:
# every short name, then every long name, and the option's own name when it
# declares no long one. Dashes are restored, and a non-flag option is followed
# by its value placeholder.
pure usage_option_names(spec: CliOption) -> Str {
  let shorts = ["-" + short for short in spec.short]
  var longs: List[Str] = []
  if spec.long.len() == 0 {
    longs = ["--" + spec.name.replace("_", "-")]
  } else {
    longs = ["--" + long.replace("_", "-") for long in spec.long]
  }
  var names = shorts.extend(longs).join(", ")
  if !spec.flag {
    let metavar = usage_metavar(spec)
    if spec.optional_value {
      names = names + f"[=${metavar}]"
    } else {
      names = names + " " + metavar
    }
  }
  return names
}

# One line of the `options:` section: the option's names, its help, and its
# deprecation note.
#
# The note is the bare word when the descriptor asked for deprecation, and it
# repeats the baseline's own message only when the descriptor wrote one.
pure option_line(spec: CliOption) -> Str {
  var line = "  " + usage_option_names(spec)
  if spec.help != null {
    line = line + "  " + (spec.help ?? "")
  }
  if spec.deprecated != null {
    line = line + "  deprecated"
    let message = spec.deprecated ?? ""
    if message != f"option `${spec.name}` is deprecated" {
      line = line + ": " + message
    }
  }
  return line
}

# The short names the specs claim, mapped to whether the claiming spec is a flag.
#
# The specs are visited in order and a name two descriptors claim belongs to the
# later one, which is the baseline's `insert` over a walk of the same order.
pure claimed_shorts(specs: List[CliOption]) -> Record {
  var claimed: Record = {}
  for spec in specs {
    for short in spec.short {
      claimed = record_with_field(claimed, short, spec.flag)
    }
  }
  return claimed
}

# Whether any spec claims `name` as a short option.
pure claims_short(claimed: Record, name: Str) -> Bool {
  let found = claimed.get(name) ?? null
  match found {
    flag is Bool => return true
    _ => return false
  }
}

# Whether the spec that claims `name` as a short option is a flag.
#
# A name no spec claims reads as `false`, which is its caller's other stopping
# condition: a scan that reads short options stops at a name that is unknown and
# at one that takes a value alike.
pure claimed_flag(claimed: Record, name: Str) -> Bool {
  let found = claimed.get(name) ?? null
  match found {
    flag is Bool => return flag
    _ => return false
  }
}

# Whether the argument list asks for help.
#
# `--help` asks under either policy. A short cluster asks when it carries an `h`
# and no spec claims `-h`, or under the strict policy, where no spec can: under
# the applet policy a descriptor that claims `-h` keeps it. Every scalar of a
# cluster is tested before the scan decides whether the cluster continues, so an
# `h` that would have been part of another option's value asks too — the
# baseline tests the scalar before it consults the spec, and the scan then walks
# the cluster only while each scalar names a flag.
#
# An argument of `--` ends the scan: what follows it is an operand, and an
# operand never asks. A negative number is an operand even when it is spelled
# with a leading `-`.
pure argv_requests_help(argv: List[Str], specs: List[CliOption], policy: Str) -> Bool {
  let claimed = claimed_shorts(specs)
  let short_help = policy == "strict" or !claims_short(claimed, "h")
  for arg in argv {
    if arg == "--" {
      return false
    }
    if arg == "--help" {
      return true
    }
    if arg.starts_with("-") and !arg.starts_with("--") and !looks_negative_number(arg) {
      let cluster = arg.byte_slice(1)
      if cluster == "h" and short_help {
        return true
      }
      if cluster.byte_len() <= 1 {
        continue
      }
      var offset = 0
      while offset < cluster.byte_len() {
        let step = utf8_step(cluster, offset)
        if step.scalar == "h" and short_help {
          return true
        }
        if !claimed_flag(claimed, step.scalar) {
          break
        }
        offset = offset + step.size
      }
    }
  }
  return false
}

# Whether a repeated occurrence of a scalar option overwrites instead of being
# rejected.
#
# The baseline's `ParsePolicy::allows_scalar_overwrite`, and the same policy
# spelling selects it everywhere else.
pure allows_scalar_overwrite(policy: Str) -> Bool {
  return policy == "applet"
}

# Render interpreted specs as usage text.
#
# The command line lists visible positionals in schema order, required ones
# bare and optional ones bracketed, and always ends with `[OPTIONS]`. The
# `arguments:` section is present only when a visible positional carries help;
# the `options:` section always follows and lists every other visible spec. The
# trailing help line names both help spellings, unless the applet policy is in
# force and some spec claims `-h`, in which case only `--help` is named.
pure usage_text(specs: List[CliOption], command: Str, policy: Str) -> Str {
  var head = "usage: " + command
  for spec in specs {
    if spec.hidden or !spec.positional {
      continue
    }
    let label = positional_label(spec)
    if spec.required {
      head = head + " " + label
    } else {
      head = head + " [" + label + "]"
    }
  }
  head = head + " [OPTIONS]"
  let visible_positionals = [
    spec for spec in specs if !spec.hidden and spec.positional and spec.help != null
  ]
  let visible_options = [spec for spec in specs if !spec.hidden and !spec.positional]
  var output = head
  if visible_positionals.len() > 0 {
    let lines = [
      "  " + usage_metavar(spec) + "  " + (spec.help ?? "") for spec in visible_positionals
    ]
    output = output + "\n\narguments:\n" + lines.join("\n")
  }
  output = output + "\n\noptions:"
  let lines = [option_line(spec) for spec in visible_options]
  if lines.len() > 0 {
    output = output + "\n" + lines.join("\n")
  }
  output = output + "\n"
  if policy == "applet" and claimed_shorts(specs).has("h") {
    return output + "  --help  show this help"
  }
  return output + "  -h, --help  show this help"
}

## Render a schema as usage text.
##
## The command line shows the command, its visible positionals in schema order,
## and `[OPTIONS]`. A positional is required and shown bare when the schema
## declares it required, and optional and bracketed otherwise. Then come the
## `arguments:` section, one line per visible positional that carries help, and
## the `options:` section, one line per visible option with its help and any
## deprecation note, and finally the two help options.
##
## Option names are given in a fixed order: short names first, then long names,
## and the schema's own name when the descriptor declares no long one, followed
## by the value placeholder for an option that takes a value. A hidden
## descriptor appears nowhere.
##
## A descriptor the schema interpreter rejects rejects the whole render: the
## call aborts with that interpreter's error, whose kind is `cli-parse`. The
## schema is read in sorted name order, so the rejection reported is the first
## one the baseline would report.
##
## The declared return type is `Any` rather than the registry's `Str` because
## that rejection is a runtime error rather than a returned value: the lowered
## runtime raises `Err` out of a function whose declared kind is not a
## `Result`, which is what keeps the rejection's kind, message, and call-site
## span identical to the baseline's native route.
export pure usage(schema: Record, command: Str = "command") -> Any {
  match schema_specs(schema, "strict") {
    Ok(specs) => return usage_text(specs, command, "strict")
    Err(failure) => return Err(failure)
  }
}

# One parse step's state: the values recorded so far, the record naming where
# each value came from, the warnings the descriptors asked for, and the set of
# names already supplied.
#
# Presence is a set of names rather than a property of a value, so an option set
# to its own default is still present for the duplicate, conflict, and required
# checks. It is a `Map` because presence is only ever tested, added, or removed,
# never visited in order: every check that reports on presence walks the specs,
# which are already in schema key order.
type CliState = {
  values: Record,
  sources: Record,
  warnings: List[Str],
  present: Map[Bool],
}

# One short-option cluster's outcome: the state it produced, and the argument
# index the caller resumes at.
type CliCursor = {state: CliState, index: Int}

# An empty presence set.
#
# The literal `{}` is a `Record`, so an empty `Map[Bool]` is built from an empty
# name list instead.
pure empty_present() -> Map[Bool] {
  let none: List[Str] = []
  let present: Map[Bool] = {name: true for name in none}
  return present
}

# The same state with one deprecation warning appended.
#
# Warnings keep the order they were raised in, so the list is extended rather
# than rebuilt, and a schema raises one per deprecated option it declares.
pure warned(state: CliState, message: Str) -> CliState {
  let next: CliState = {
    values: state.values,
    sources: state.sources,
    warnings: state.warnings.push(message),
    present: state.present,
  }
  return next
}

# The same state with `name` marked as supplied.
#
# An option records its presence where it records its value, and a positional
# records it as it consumes the operand, so the two paths share this.
pure seen(state: CliState, name: Str) -> CliState {
  let next: CliState = {
    values: state.values,
    sources: state.sources,
    warnings: state.warnings,
    present: state.present.set(name, true),
  }
  return next
}

# The integer text the baseline's `parse::<i64>` accepts, or null.
#
# `Int` and `UInt` values are parsed with the baseline's `str::parse::<i64>`,
# which accepts an optional leading `+` or `-`, then one or more ASCII digits,
# and nothing else: no white space, no radix prefix, no `_` separator, and
# leading zeros are allowed. A value outside `Int` range fails, including the
# smallest `Int` spelling, which is reported here rather than built by the
# multiplication below. `Str.parse_int` accepts more than that and rejects that
# spelling, because it reads the magnitude before it applies the sign, so the
# digits are accumulated here under the bounds of `Int`; the accumulation cannot
# trap, because a digit is only multiplied once the value is known to be in
# range. `stdlib/system.xsh` parses the same spelling under the name
# `parse_field_int`.
pure strict_int(text: Str) -> Int? {
  let end = text.byte_len()
  if end == 0 {
    return null
  }
  # 43 is `+` and 45 is `-`.
  let first = text.byte_at(0, 0)
  var start = 0
  var negative = false
  if first == 43 or first == 45 {
    start = 1
    negative = first == 45
  }
  if start == end {
    return null
  }
  var value = 0
  var index = start
  while index < end {
    let byte = text.byte_at(index, 0)
    if byte < 48 or byte > 57 {
      return null
    }
    let digit = byte - 48
    # 922337203685477580 is `Int` max without its last digit, so any value above
    # it is out of range on the next step.
    if value > 922337203685477580 {
      return null
    }
    if value == 922337203685477580 {
      if digit == 8 and negative and index + 1 == end {
        return 0 - 9223372036854775807 - 1
      }
      if digit > 7 {
        return null
      }
    }
    value = value * 10 + digit
    index = index + 1
  }
  if negative {
    return 0 - value
  }
  return value
}

# The text a value contributes to a `choices` comparison, or null when the value
# cannot take part in one.
#
# The baseline renders an `Int` and a `Bool` with their own spellings, a `Path`
# by its display text, and a `Duration` as a millisecond count; any other type
# is reported to the caller as a value that cannot use `choices` at all.
pure value_choice_text(value: Any) -> Str? {
  match value {
    word is Str => return word
    count is Int => return f"${count}"
    flag is Bool => return f"${flag}"
    target is Path => return target.display()
    span is Duration => return f"${span}"
    _ => return null
  }
}

# The label a rejection names one spec by, and the name a usage line shows.
#
# A positional is named by its form with any leading `...` removed, and by its
# schema name when it declares no form; anything else prefers its first long
# name, then its first short name, and falls back to its schema name with
# underscores spelled as dashes.
pure option_label(spec: CliOption) -> Str {
  if spec.positional {
    if spec.form == null {
      return spec.name
    }
    return trim_leading_ellipsis(spec.form ?? "")
  }
  if spec.long.len() > 0 {
    return "--" + spec.long.get(0, "").replace("_", "-")
  }
  if spec.short.len() > 0 {
    return "-" + spec.short.get(0, "")
  }
  return "--" + spec.name.replace("_", "-")
}

# The spec a schema names, or null when no spec carries that name.
#
# Specs are held in schema key order, and their names are the schema's own keys,
# so a name matches at most one spec.
pure spec_named(specs: List[CliOption], name: Str) -> CliOption? {
  for spec in specs {
    if spec.name == name {
      return spec
    }
  }
  return null
}

# Map every long name to the spec that declares it, rejecting an empty or a
# duplicated one.
#
# Names come from form descriptors, already normalized, and the specs are
# visited in schema key order, so the rejection reported is the baseline's first.
pure long_specs_map(specs: List[CliOption]) -> Result[Record] {
  var mapped: Record = {}
  for spec in specs {
    for long in spec.long {
      if long == "" {
        return Err(cli_parse_error(f"option `${spec.name}` has an empty long name"))
      }
      if mapped.has(long) {
        return Err(cli_parse_error(f"duplicate long option `--${long}`"))
      }
      mapped = record_with_field(mapped, long, spec.name)
    }
  }
  return Ok(mapped)
}

# Map every short name to the spec that declares it, rejecting an empty or a
# duplicated one.
pure short_specs_map(specs: List[CliOption]) -> Result[Record] {
  var mapped: Record = {}
  for spec in specs {
    for short in spec.short {
      if short == "" {
        return Err(cli_parse_error(f"option `${spec.name}` has an empty short name"))
      }
      if mapped.has(short) {
        return Err(cli_parse_error(f"duplicate short option `-${short}`"))
      }
      mapped = record_with_field(mapped, short, spec.name)
    }
  }
  return Ok(mapped)
}

# The boolean a text spells, or null when it spells none.
#
# The baseline's set is exactly `1`, `true`, `yes`, and `on` for true, and `0`,
# `false`, `no`, and `off` for false. Spellings are compared as written, so `True`
# is not a boolean here.
pure parse_bool(raw: Str) -> Bool? {
  if raw == "1" or raw == "true" or raw == "yes" or raw == "on" {
    return true
  }
  if raw == "0" or raw == "false" or raw == "no" or raw == "off" {
    return false
  }
  return null
}

# Convert one text to a declared value type, or reject it.
#
# `Str` is taken as written, `Int` and `UInt` are parsed by spelling with `UInt`
# a non-negative `Int`, and `Bool` takes the baseline's eight spellings. `Path`
# and `Duration` are the two types with a text spelling of their own: a path
# with a NUL byte reports its own `nul-path` kind rather than a `cli-parse`
# rejection, and a duration that does not parse is a `cli-parse` rejection. A
# rejection names the option and the argument index the value came from, which
# for an inline or clustered value is the option's own index.
pure convert_arg_value(name: Str, raw: Str, ty: Str, index: Int) -> Result[CliValue] {
  if ty == "Str" {
    return Ok({value: raw})
  }
  if ty == "Int" {
    let parsed = strict_int(raw)
    if parsed == null {
      return Err(cli_parse_error(f"option --${name} expects Int at argv[${index}], got `${raw}`"))
    }
    return Ok({value: parsed ?? 0})
  }
  if ty == "UInt" {
    let parsed = strict_int(raw)
    if parsed == null or (parsed ?? 0) < 0 {
      return Err(cli_parse_error(f"option --${name} expects UInt at argv[${index}], got `${raw}`"))
    }
    return Ok({value: parsed ?? 0})
  }
  if ty == "Bool" {
    let parsed = parse_bool(raw)
    if parsed == null {
      return Err(cli_parse_error(f"option --${name} expects Bool at argv[${index}], got `${raw}`"))
    }
    return Ok({value: parsed ?? false})
  }
  if ty == "Path" {
    return Ok({value: path_from_text(raw)?})
  }
  let span = duration_from_literal(raw)
  if span == null {
    return Err(cli_parse_error(
      f"option --${name} expects Duration at argv[${index}], got `${raw}`"
    ))
  }
  return Ok({value: span})
}

# Whether the path's target is reachable.
#
# The baseline asks `Path::exists`, which follows symbolic links: a link whose
# target is missing is not an existing path, while the path itself is present.
# The link case is the only reason this needs a second probe, so the extra
# resolution happens only after a symbolic link is actually observed.
proc path_exists(target: Path) [fs] -> Bool {
  if !(fs.exists(target) ?? false) {
    return false
  }
  match target.metadata() {
    Ok(entry) => {
      if entry.kind != "symlink" {
        return true
      }
    }
    Err(_) => {
      return false
    }
  }
  match target.resolve() {
    Ok(_) => return true
    Err(_) => return false
  }
}

# The kind of the path's target, following symbolic links.
#
# The baseline asks `Path::is_file` and `Path::is_dir`, which inspect the link's
# target rather than the link. An empty answer means the target cannot be
# inspected at all, which satisfies neither question.
proc path_target_kind(target: Path) [fs] -> Str {
  match target.metadata() {
    Ok(entry) => {
      if entry.kind != "symlink" {
        return entry.kind
      }
    }
    Err(_) => {
      return ""
    }
  }
  match target.resolve() {
    Ok(followed) => {
      match followed.metadata() {
        Ok(entry) => return entry.kind
        Err(_) => return ""
      }
    }
    Err(_) => return ""
  }
}

# Reject a path value the descriptor requires the file system to confirm.
#
# The baseline asks in this order — an existing path, then a file, then a
# directory — so a value that fails several requirements is reported against the
# first of them, and a descriptor that asks for none of the three probes nothing.
proc validate_path_option(spec: CliOption, target: Path) [fs] -> Result[Unit] {
  if !(spec.exists or spec.file or spec.dir) {
    return Ok()
  }
  let label = option_label(spec)
  let display = target.display()
  if spec.exists and !path_exists(target) {
    return Err(cli_parse_error(f"option ${label} expects an existing path: ${display}"))
  }
  if spec.file or spec.dir {
    let kind = path_target_kind(target)
    if spec.file and kind != "file" {
      return Err(cli_parse_error(f"option ${label} expects a file path: ${display}"))
    }
    if spec.dir and kind != "dir" {
      return Err(cli_parse_error(f"option ${label} expects a directory path: ${display}"))
    }
  }
  return Ok()
}

# Reject a value that violates its descriptor's constraints.
#
# `choices` compares the value's own spelling, and rejects a value that has no
# spelling at all. `positive`, `nonzero`, `min`, and `max` apply to an `Int`,
# and a `positive` `Duration` is one that is not zero.
#
# `exists`, `file`, and `dir` consult the file system, and only for a `Path`
# value. Those probes are the baseline's one legacy exception in this policy:
# the public entries are declared pure while their path constraints read host
# state, so they are performed here, at the point the baseline performs them —
# inside value validation, in declaration order, so a rejected value names the
# same argument the baseline names first — and the value walk that reaches them
# declares the effect it really has instead of hiding it.
proc validate_option_value(spec: CliOption, value: Any, index: Int) [fs] -> Result[Unit] {
  let label = option_label(spec)
  if spec.choices.len() > 0 {
    let text = value_choice_text(value)
    if text == null {
      return Err(cli_parse_error(
        f"option ${label} cannot use `choices` with ${type_name(value)}"
      ))
    }
    if !spec.choices.contains(text ?? "") {
      return Err(cli_parse_error(
        f"option ${label} expects one of ${spec.choices.join("|")}, got `${text ?? ""}` at argv[${index}]"
      ))
    }
  }
  match value {
    count is Int => {
      if spec.positive and count <= 0 {
        return Err(cli_parse_error(f"option ${label} expects a positive integer"))
      }
      if spec.nonzero and count == 0 {
        return Err(cli_parse_error(f"option ${label} expects a non-zero integer"))
      }
      let min = spec.min ?? 0
      if spec.min != null and count < min {
        return Err(cli_parse_error(f"option ${label} expects value >= ${min}"))
      }
      let max = spec.max ?? 0
      if spec.max != null and count > max {
        return Err(cli_parse_error(f"option ${label} expects value <= ${max}"))
      }
    }
    span is Duration => {
      if spec.positive and span == time.millis(0) {
        return Err(cli_parse_error(f"option ${label} expects a positive duration"))
      }
    }
    target is Path => {
      validate_path_option(spec, target)?
    }
    _ => {}
  }
  return Ok()
}

# The value an option whose value may be omitted takes.
#
# A descriptor that declares one supplies it, a `Bool` option takes `true`, and
# any other option takes its own default. An option with none of the three
# rejects the argument list, because a value was promised and none is present.
pure optional_value_default(spec: CliOption) -> Result[CliValue] {
  if spec.optional_default_value != null {
    return Ok({value: spec.optional_default_value})
  }
  if spec.value_ty == "Bool" {
    return Ok({value: true})
  }
  if spec.default_value != null {
    return Ok({value: spec.default_value})
  }
  return Err(cli_parse_error(f"option ${option_label(spec)} expects a value"))
}

# The text an environment fallback contributes, or a rejection naming the type
# the environment carried instead.
#
# The baseline accepts exactly the scalar types an option value can take, and
# reports any other value rather than ignoring the fallback.
pure value_to_env_text(name: Str, value: Any) -> Result[Str] {
  match value {
    word is Str => return Ok(word)
    count is Int => return Ok(f"${count}")
    flag is Bool => return Ok(f"${flag}")
    target is Path => return Ok(target.display())
    span is Duration => return Ok(f"${span}")
    _ => return Err(cli_parse_error(
      f"env fallback for option `${name}` must be scalar, found ${type_name(value)}"
    ))
  }
}

# The value and source each spec starts with, before any argument is read.
#
# A repeated option always has a value: its declared default, or an empty list.
# Otherwise an environment name the environment record carries supplies the
# value — converted, validated, and reported as `env` — then the declared
# default, then `false` for a flag, then `null` for an optional option that
# declares nothing. A required option with no value records nothing at all, so
# the required check after the walk is what reports it.
#
# The caller's record is bound as `environ` and never as `env`: `env` is also a
# standard module name, and a call spelled `env.get(...)` resolves to that
# module's own process-environment lookup rather than to this record's method,
# silently ignoring the record the caller passed. Only the modules a file
# mentions are prepared, so the collision is invisible until the call runs.
#
# A declared default is validated exactly like a spelled one, so the path
# constraints reach the file system here too.
proc defaults(specs: List[CliOption], environ: Record) [fs] -> Result[CliState] {
  var values: Record = {}
  var sources: Record = {}
  for spec in specs {
    let name = spec.name
    if spec.repeated {
      if spec.default_value == null {
        values = record_with_field(values, name, [])
        sources = record_with_field(sources, name, "absent")
      } else {
        values = record_with_field(values, name, spec.default_value)
        sources = record_with_field(sources, name, "default")
      }
      continue
    }
    let env_name = spec.env
    if env_name != null and environ.has(env_name ?? "") {
      let raw = value_to_env_text(name, environ.get(env_name ?? "") ?? null)?
      let converted = convert_arg_value(name, raw, spec.value_ty, 0)?
      validate_option_value(spec, converted.value, 0)?
      values = record_with_field(values, name, converted.value)
      sources = record_with_field(sources, name, "env")
      continue
    }
    if spec.default_value != null {
      validate_option_value(spec, spec.default_value, 0)?
      values = record_with_field(values, name, spec.default_value)
      sources = record_with_field(sources, name, "default")
      continue
    }
    if spec.flag {
      values = record_with_field(values, name, false)
      sources = record_with_field(sources, name, "default")
      continue
    }
    if !spec.required {
      values = record_with_field(values, name, null)
      sources = record_with_field(sources, name, "absent")
    }
  }
  let state: CliState = {
    values: values,
    sources: sources,
    warnings: [],
    present: empty_present(),
  }
  return Ok(state)
}

# The argument name a spelled long name resolves to.
#
# The long-name map is keyed by normalized names, so `--dry-run`, `--dry_run`,
# and a descriptor that spells its long name either way all resolve to the same
# spec. A name no spec declares resolves to its own normalized spelling, which
# is what the lookup that follows rejects.
pure resolve_long_name(long_specs: Record, raw_name: Str) -> Str {
  let key = normalize_arg_name(raw_name)
  let mapped = long_specs.get(key) ?? null
  match mapped {
    found is Str => return found
    _ => return key
  }
}

# The spec a short name resolves to, or the rejection an unknown name gets.
#
# The short-name map is built from the specs, so a name it carries always names
# a spec; a name it does not carry is unknown, even when a spec is named after
# that single character.
pure resolve_short(
  specs: List[CliOption],
  short_specs: Record,
  short_name: Str,
  index: Int
) -> Result[CliOption] {
  let mapped = short_specs.get(short_name) ?? null
  match mapped {
    found is Str => {
      for spec in specs {
        if spec.name == found {
          return Ok(spec)
        }
      }
    }
    _ => {}
  }
  return Err(cli_parse_error(f"unknown argument at argv[${index}]: -${short_name}"))
}

# Record one option's value.
#
# The duplicate check rejects a second value for an option that does not repeat,
# unless the policy allows a scalar to be overwritten. Before either, the applet
# policy resets the options this spec declares a conflict with: each one that is
# present takes its default — `false` for a flag and `null` otherwise — and stops
# counting as supplied, so the later option wins rather than the pair being
# reported. A repeated option appends to its list, and any other option
# overwrites its default in place. The source of every value recorded here is
# `argv`.
proc set_option_value(
  state: CliState,
  specs: List[CliOption],
  spec: CliOption,
  value: Any,
  index: Int,
  policy: Str
) [fs] -> Result[CliState] {
  var values = state.values
  var present = state.present
  if allows_scalar_overwrite(policy) {
    for conflict in spec.conflicts {
      let key = normalize_arg_name(conflict)
      if !present.has(key) {
        continue
      }
      let named = spec_named(specs, key)
      if named == null {
        continue
      }
      let other = named ?? unrendered_spec()
      var reset: Any = null
      if other.default_value != null {
        reset = other.default_value
      } else if other.flag {
        reset = false
      }
      values = record_with_field(values, key, reset)
      present = present.remove(key)
    }
  }
  if !allows_scalar_overwrite(policy) and !spec.repeated and present.has(spec.name) {
    return Err(cli_parse_error(f"duplicate argument at argv[${index}]: --${spec.name}"))
  }
  validate_option_value(spec, value, index)?
  if spec.repeated {
    let current = values.get(spec.name) ?? null
    match current {
      items is List[Any] => {
        values = record_with_field(values, spec.name, items.push(value))
      }
      _ => {
        values = record_with_field(values, spec.name, [value])
      }
    }
  } else {
    values = record_with_field(values, spec.name, value)
  }
  let next: CliState = {
    values: values,
    sources: record_with_field(state.sources, spec.name, "argv"),
    warnings: state.warnings,
    present: present.set(spec.name, true),
  }
  return Ok(next)
}

# Consume one short-option cluster, starting at the argument `index` names.
#
# Every scalar of the cluster is a short name: an unknown name is a rejection, a
# flag records `true` and moves on, and the first name that takes a value
# consumes the rest of the cluster, or the next argument when the cluster ends
# there. The returned index is where the caller resumes: past the cluster, past
# the consumed value, and past the argument the value came from.
proc parse_short_options(
  argv: List[Str],
  specs: List[CliOption],
  short_specs: Record,
  state: CliState,
  index: Int,
  policy: Str
) [fs] -> Result[CliCursor] {
  let cluster = strip_leading_dashes(argv.get(index, ""))
  var current = state
  var offset = 0
  while offset < cluster.byte_len() {
    let step = utf8_step(cluster, offset)
    let spec = resolve_short(specs, short_specs, step.scalar, index)?
    if !allows_scalar_overwrite(policy) and !spec.repeated and current.present.has(spec.name) {
      return Err(cli_parse_error(f"duplicate argument at argv[${index}]: -${step.scalar}"))
    }
    if spec.flag {
      current = set_option_value(current, specs, spec, true, index, policy)?
      if spec.deprecated != null {
        current = warned(current, spec.deprecated ?? "")
      }
      offset = offset + step.size
      continue
    }
    var raw_value = ""
    var raw_index = index
    var consumed = 0
    let after = offset + step.size
    if after < cluster.byte_len() {
      raw_value = cluster.byte_slice(after)
    } else {
      let next = argv.get(index + 1, null)
      match next {
        text is Str => {
          if text.starts_with("--") {
            return Err(cli_parse_error(
              f"missing value for -${step.scalar} at argv[${index}]"
            ))
          }
          raw_value = text
          raw_index = index + 1
          consumed = 1
        }
        _ => return Err(cli_parse_error(
          f"missing value for -${step.scalar} at argv[${index}]"
        ))
      }
    }
    let converted = convert_arg_value(spec.name, raw_value, spec.value_ty, raw_index)?
    current = set_option_value(current, specs, spec, converted.value, index, policy)?
    if spec.deprecated != null {
      current = warned(current, spec.deprecated ?? "")
    }
    let cursor: CliCursor = {state: current, index: index + consumed + 1}
    return Ok(cursor)
  }
  let cursor: CliCursor = {state: current, index: index + 1}
  return Ok(cursor)
}

# Record one positional's value.
#
# A repeated positional appends to the list its default started, and any other
# positional overwrites its default; the source of a value read from an argument
# is always `argv`. The caller records the name as supplied.
proc push_positional_value(
  state: CliState,
  spec: CliOption,
  raw: Str,
  index: Int
) [fs] -> Result[CliState] {
  let converted = convert_arg_value(spec.name, raw, spec.value_ty, index)?
  validate_option_value(spec, converted.value, index)?
  var values = state.values
  if spec.repeated {
    let current = values.get(spec.name) ?? null
    match current {
      items is List[Any] => {
        values = record_with_field(values, spec.name, items.push(converted.value))
      }
      _ => {
        values = record_with_field(values, spec.name, [converted.value])
      }
    }
  } else {
    values = record_with_field(values, spec.name, converted.value)
  }
  let next: CliState = {
    values: values,
    sources: record_with_field(state.sources, spec.name, "argv"),
    warnings: state.warnings,
    present: state.present,
  }
  return Ok(next)
}

# Reject an argument list that violates a relationship the schema declares.
#
# Conflicts and requirements are checked in schema key order, and a present
# option reports its first violated relationship, so the rejection a caller sees
# is the one the baseline reports. A required group is reported only when none
# of its members is present, and its members are listed in schema key order,
# which is the order the baseline collects them in.
#
# A relationship that names a spec the schema does not declare falls back to the
# spec being checked, which is what the baseline's `unwrap_or` does: such a name
# can never be present, so a `requires` on it always rejects and a `conflicts`
# with it never does.
pure validate_relationships(specs: List[CliOption], present: Map[Bool]) -> Result[Unit] {
  var group_names: Record = {}
  for spec in specs {
    if spec.required_group != null {
      group_names = record_with_field(group_names, spec.required_group ?? "", true)
    }
    if !present.has(spec.name) {
      continue
    }
    for conflict in spec.conflicts {
      let key = normalize_arg_name(conflict)
      if !present.has(key) {
        continue
      }
      let other = spec_named(specs, key)
      return Err(cli_parse_error(
        f"${option_label(spec)} conflicts with ${option_label(other ?? spec)}"
      ))
    }
    for required in spec.requires {
      let key = normalize_arg_name(required)
      if present.has(key) {
        continue
      }
      let other = spec_named(specs, key)
      return Err(cli_parse_error(
        f"${option_label(spec)} requires ${option_label(other ?? spec)}"
      ))
    }
  }
  for group in group_names.keys() {
    let members = [spec for spec in specs if (spec.required_group ?? "") == group]
    let supplied = [spec for spec in members if present.has(spec.name)].len() > 0
    if supplied {
      continue
    }
    let labels = [option_label(spec) for spec in members].join(", ")
    return Err(cli_parse_error(f"one of required group `${group}` is required: ${labels}"))
  }
  return Ok()
}

# Parse an argument list against interpreted specs.
#
# The walk is the baseline's. `--` ends option parsing, and everything after it
# is positional. A long option may carry its value inline, takes the next
# argument when its descriptor says its value may be omitted and the next
# argument is an option, and otherwise requires one. A short option clusters
# flags and carries its value in the rest of the cluster. Anything else is an
# operand, and an operand with no positional left to fill is a rejection.
#
# The policy decides whether a second occurrence of a scalar option is a
# duplicate: the applet policy overwrites, and the strict policy rejects.
#
# Every rejection is a `cli-parse` error. Attaching the usage text to one is
# entry-point policy, so a caller that reports a rejection with usage text —
# `parse_entry` below, for `cli.parse`, `cli.parse_full`, and `cli.applet` —
# does that itself.
proc parse_values(
  argv: List[Str],
  specs: List[CliOption],
  environ: Record,
  policy: Str
) [fs] -> Result[CliState] {
  let positionals = [spec for spec in specs if spec.positional]
  let long_specs = long_specs_map(specs)?
  let short_specs = short_specs_map(specs)?
  var state = defaults(specs, environ)?
  var index = 0
  var positional_index = 0
  while index < argv.len() {
    let token = argv.get(index, "")
    if token == "--" {
      # Everything after `--` is an operand, and the walk ends with the list.
      var offset = index + 1
      while offset < argv.len() {
        let raw = argv.get(offset, "")
        if positional_index >= positionals.len() {
          return Err(cli_parse_error(
            f"unexpected positional argument at argv[${offset}]: ${raw}"
          ))
        }
        # The index check above is what rejects an operand with no positional
        # left, so the placeholder keeps the lookup total.
        let spec = positionals.get(positional_index, unrendered_spec())
        if !spec.repeated and state.present.has(spec.name) {
          return Err(cli_parse_error(
            f"duplicate positional argument at argv[${offset}]: ${raw}"
          ))
        }
        state = push_positional_value(state, spec, raw, offset)?
        state = seen(state, spec.name)
        if !spec.repeated {
          positional_index = positional_index + 1
        }
        offset = offset + 1
      }
      break
    }
    if token.starts_with("--") {
      let option = token.byte_slice(2)
      if option == "" {
        return Err(cli_parse_error(f"empty option at argv[${index}]"))
      }
      let parts = option.split("=", 1)
      let raw_name = parts.get(0, "")
      var inline: Str? = null
      if parts.len() == 2 {
        inline = parts.get(1, "")
      }
      let peek = argv.get(index + 1, null)
      var next_looks_like_option = true
      match peek {
        text is Str => {
          next_looks_like_option = text.starts_with("-")
        }
        _ => {}
      }
      let name = resolve_long_name(long_specs, raw_name)
      let found = spec_named(specs, name)
      if found == null {
        return Err(cli_parse_error(f"unknown argument at argv[${index}]: --${raw_name}"))
      }
      # The lookup above is what rejects an unknown name, so the placeholder is
      # unreachable and only keeps the spec's type known.
      let spec = found ?? unrendered_spec()
      if !allows_scalar_overwrite(policy) and !spec.repeated and state.present.has(name) {
        return Err(cli_parse_error(f"duplicate argument at argv[${index}]: --${raw_name}"))
      }
      var value: Any = null
      var consumed = 0
      if spec.flag and inline == null {
        value = true
      } else if inline != null {
        let converted = convert_arg_value(name, inline ?? "", spec.value_ty, index)?
        value = converted.value
      } else if spec.optional_value and next_looks_like_option {
        let converted = optional_value_default(spec)?
        value = converted.value
      } else {
        let next = argv.get(index + 1, null)
        match next {
          text is Str => {
            if text.starts_with("--") {
              return Err(cli_parse_error(
                f"missing value for --${raw_name} at argv[${index}]"
              ))
            }
            let converted = convert_arg_value(name, text, spec.value_ty, index + 1)?
            value = converted.value
            consumed = 1
          }
          _ => return Err(cli_parse_error(
            f"missing value for --${raw_name} at argv[${index}]"
          ))
        }
      }
      state = set_option_value(state, specs, spec, value, index, policy)?
      if spec.deprecated != null {
        state = warned(state, spec.deprecated ?? "")
      }
      index = index + consumed + 1
      continue
    }
    if token.starts_with("-") and token.byte_len() > 1 and !looks_negative_number(token) {
      let cursor = parse_short_options(argv, specs, short_specs, state, index, policy)?
      state = cursor.state
      index = cursor.index
      continue
    }
    if positional_index >= positionals.len() {
      return Err(cli_parse_error(
        f"unexpected positional argument at argv[${index}]: ${token}"
      ))
    }
    # The length check above is what rejects an operand with no positional left,
    # so the placeholder keeps the lookup total.
    let spec = positionals.get(positional_index, unrendered_spec())
    if !spec.repeated and state.present.has(spec.name) {
      return Err(cli_parse_error(
        f"duplicate positional argument at argv[${index}]: ${token}"
      ))
    }
    state = push_positional_value(state, spec, token, index)?
    state = seen(state, spec.name)
    if !spec.repeated {
      positional_index = positional_index + 1
    }
    index = index + 1
  }
  for spec in specs {
    if spec.required and !state.present.has(spec.name) {
      return Err(cli_parse_error(f"missing required argument ${option_label(spec)}"))
    }
  }
  validate_relationships(specs, state.present)?
  return Ok(state)
}

# The shared body of the argument-list entries.
#
# The schema is interpreted first, so a reserved name stays a schema rejection;
# then a help request is honored with the usage text as the message of a
# `cli-help` rejection; then the walk runs, and a rejection it produces is
# reported with the usage text appended and the `cli_usage` flag set. The
# rejection's own kind is carried through, so a failure that did not come from
# the walk — a `nul-path` value — is reported under its own kind.
#
# The environment is exactly the record the caller passed, and `cli.parse` and
# `cli.applet` pass the empty one.
proc parse_entry(
  argv: List[Str],
  schema: Record,
  environ: Record,
  command: Str,
  policy: Str
) [fs] -> Result[CliState] {
  let specs = schema_specs(schema, policy)?
  let usage = usage_text(specs, command, policy)
  if argv_requests_help(argv, specs, policy) {
    return Err(cli_help_error(usage))
  }
  match parse_values(argv, specs, environ, policy) {
    Ok(parsed) => return Ok(parsed)
    Err(failure) => return Err(cli_usage_error(failure.kind, failure.message, usage))
  }
}

## Parse an argument list against a schema of option descriptors.
##
## The result is a record with one field per schema entry: the value spelled on
## the command line, the descriptor's default, the value an environment name it
## declares supplies, `false` for a flag, or `null` for an optional entry that
## declares no value. An entry the schema requires and no argument supplies is a
## rejection, and so is an argument the schema has no place for. The fields are
## present whether or not an argument supplied them, and iterating them visits
## the schema's names in sorted order.
##
## `--help`, `-h`, or any short cluster carrying `h`, asks for help: the call
## rejects with `cli-help` and the rendered usage text as its message. A value
## the schema refuses rejects with `cli-parse`, the usage text appended to the
## message, and the `cli_usage` field set; a descriptor the interpreter cannot
## read rejects with `cli-parse` and no usage text. An unhandled rejection of the
## last two kinds is what the outer CLI boundary prints on stderr with status 2,
## and an unhandled help request is what it prints on stdout with status 0.
##
## The usage text is labeled with `command`, which defaults to the name this
## program was invoked under.
export proc parse(
  argv: List[Str],
  schema: Record,
  command: Str? = null
) [fs] -> Result[Record] {
  let parsed = parse_entry(argv, schema, {}, command ?? command_name(), "strict")?
  return Ok(sorted_record(parsed.values))
}

## Parse an argument list against a schema of option descriptors, reporting
## where every value came from.
##
## The result carries `values`, the record `parse` returns; `sources`, the same
## fields naming the origin of each value (`"argv"`, `"env"`, `"default"`, or
## `"absent"`); and `warnings`, the deprecation messages of the deprecated
## descriptors that were used, in the order they were used.
##
## `env` is the record consulted for the environment name a descriptor declares;
## a name it does not carry falls back to the descriptor's default. Every other
## rule — the schema, the help request, the rejection with usage text, and the
## `command` default — is `parse`'s.
export proc parse_full(
  argv: List[Str],
  schema: Record,
  env: Record = {},
  command: Str? = null
) [fs] -> Result[Record] {
  let parsed = parse_entry(argv, schema, env, command ?? command_name(), "strict")?
  var report: Record = record_with_field({}, "sources", parsed.sources)
  report = record_with_field(report, "values", parsed.values)
  report = record_with_field(report, "warnings", parsed.warnings)
  return Ok(sorted_record(report))
}

## Parse an argument list the way an applet does.
##
## An applet owns its command line, so three rules differ from `parse`. A
## descriptor may claim `-h` as a short name, and `-h` then reaches that
## descriptor instead of asking for help; the usage text names only `--help` as
## the help option, since `-h` is no longer one. An option whose descriptor
## declares `conflicts` with an option already supplied resets that option to
## its default and drops it from the supplied set instead of rejecting the
## command line. A second occurrence of a scalar option overwrites the first
## instead of being rejected as a duplicate.
##
## `--help` always asks for help, and every other rule — the schema, the value
## rejection with usage text, and the `command` default — is `parse`'s.
export proc applet(
  argv: List[Str],
  schema: Record,
  command: Str? = null
) [fs] -> Result[Record] {
  let parsed = parse_entry(argv, schema, {}, command ?? command_name(), "applet")?
  return Ok(sorted_record(parsed.values))
}

# The name a command token is looked up by.
#
# A dashed spelling and an underscored one are the same command, so `my-cmd` is
# looked up as `my_cmd`. The canonical names a schema declares are stored as
# written, so a dashed canonical name is reachable only through an alias.
pure command_key(command: Str) -> Str {
  return command.replace("-", "_")
}

# Whether a token can name a command at all.
#
# A token that begins with `/` or `.`, or that contains a `/`, names a path, and
# the baseline never routes one to a fallback command that asks to be
# command-like.
pure command_like(command: Str) -> Bool {
  return !command.starts_with("/") and !command.starts_with(".") and !command.contains("/")
}

# The arguments after the command token.
#
# The baseline hands the routing walk a slice of its own argument list, so the
# walk indexes its own list from zero; a list here is copied instead of sliced.
pure argv_tail(argv: List[Str], start: Int) -> List[Str] {
  return [argv.get(i, "") for i in range(start, argv.len())]
}

# A `Bool` command descriptor field.
#
# The baseline's command reader is its option reader with the `command` lead
# word and the `cli-commands` kind, so the field is read by the shared reader and
# a rejection is re-raised under the command kind.
pure command_field_bool(fields: Record, owner: Str, field: Str, fallback: Bool) -> Result[Bool] {
  match field_bool(fields, "command", owner, field, fallback) {
    Ok(flag) => return Ok(flag)
    Err(failure) => return Err(cli_commands_error(failure.message))
  }
}

# A `Str` command descriptor field, absent when the field is absent.
pure command_field_string(fields: Record, owner: Str, field: Str) -> Result[CliText] {
  match field_string(fields, "command", owner, field) {
    Ok(read) => return Ok(read)
    Err(failure) => return Err(cli_commands_error(failure.message))
  }
}

# A `List[Str]` command descriptor field, empty when the field is absent.
#
# A field that is not a list and an element that is not a string report the same
# message, which is what the baseline's command reader does: there is no `found`
# clause and no single-string spelling of the field.
pure command_field_strings(fields: Record, owner: Str, field: Str) -> Result[List[Str]] {
  if !fields.has(field) {
    return Ok([])
  }
  let raw = fields.get(field) ?? null
  match raw {
    items is List[Any] => {
      var words: List[Str] = []
      for item in items {
        match item {
          word is Str => {
            words = words.push(word)
          }
          _ => return Err(cli_commands_error(
            f"command `${owner}` descriptor field `${field}` must be List[Str]"
          ))
        }
      }
      return Ok(words)
    }
    _ => return Err(cli_commands_error(
      f"command `${owner}` descriptor field `${field}` must be List[Str]"
    ))
  }
}

# A non-negative `Int` command descriptor field, `fallback` when the field is
# absent.
#
# The baseline reads this field through an unsigned type, so a negative value is
# its own rejection rather than a cast.
pure command_field_count(fields: Record, owner: Str, field: Str, fallback: Int) -> Result[Int] {
  if !fields.has(field) {
    return Ok(fallback)
  }
  let raw = fields.get(field) ?? null
  match raw {
    count is Int => {
      if count < 0 {
        return Err(cli_commands_error(
          f"command `${owner}` descriptor field `${field}` cannot be negative"
        ))
      }
      return Ok(count)
    }
    _ => return Err(cli_commands_error(
      f"command `${owner}` descriptor field `${field}` must be Int, found ${type_name(raw)}"
    ))
  }
}

# One command positional's declared type, with a name the command does not type
# taking `Str`.
pure command_positional_type(spec: CliCommand, name: Str) -> Str {
  match spec.types.get(name) {
    Ok(value) => {
      match value {
        word is Str => return word
        _ => return "Str"
      }
    }
    Err(_) => return "Str"
  }
}

# One command positional's type spelling, or the baseline's rejection.
#
# A `List` type is rejected by name and any other unrecognised spelling is
# reported as an unsupported command positional type; both are read from the
# trimmed spelling, which is the spelling the type is kept as.
pure command_type_name(ty: Str, command: Str, field: Str) -> Result[Str] {
  let trimmed = ty.trim()
  if trimmed.starts_with("List[") {
    return Err(cli_commands_error(
      f"command `${command}` type for `${field}` cannot be List"
    ))
  }
  let scalar = parse_scalar(trimmed)
  if scalar == null {
    return Err(cli_commands_error(f"unsupported command positional type `${trimmed}`"))
  }
  return Ok(scalar ?? "Str")
}

# A command's `types` field, as a record from positional name to type spelling.
#
# The fields are read in sorted name order, so a command that mistypes two
# positionals is reported against the baseline's first.
pure command_types(command: Str, raw: Any) -> Result[CliTypeMap] {
  match raw {
    fields is Record => {
      var mapped: Record = {}
      for field in sorted_names(fields.keys()) {
        let value = fields.get(field) ?? null
        match value {
          word is Str => {
            mapped = record_with_field(mapped, field, command_type_name(word, command, field)?)
          }
          _ => return Err(cli_commands_error(
            f"command `${command}` type for `${field}` must be Str"
          ))
        }
      }
      let parsed: CliTypeMap = {types: mapped}
      return Ok(parsed)
    }
    _ => return Err(cli_commands_error(f"command `${command}` field `types` must be Record"))
  }
}

# The positional names a command form declares, appended to those the descriptor
# declares.
#
# The form's first word is the command's own name and is skipped; a token that
# begins with `-` is one of the command's options and is skipped too; a `...NAME`
# token names the rest. Every name is lower-cased, which is the spelling the
# baseline keeps.
pure command_form(form: Str, declared: List[Str]) -> CliCommandForm {
  let tokens = form.words()
  var positionals = declared
  var rest: Str? = null
  var index = 0
  for token in tokens {
    if index > 0 {
      if !token.starts_with("-") {
        if token.starts_with("...") {
          rest = trim_leading_ellipsis(token).lower()
        } else {
          positionals = positionals.push(token.lower())
        }
      }
    }
    index = index + 1
  }
  let parsed: CliCommandForm = {positionals: positionals, rest: rest}
  return parsed
}

# Interpret one command descriptor: the baseline's `parse_command_descriptor`.
#
# The order of the steps carries meaning. The declared positionals and rest are
# read first, then the form, which appends its names and wins for the rest
# name; `min_rest`, `command_like`, and the aliases follow in that order, and the
# types and the option schema last, so a descriptor that mistypes several fields
# is reported against the baseline's first.
pure command_descriptor(name: Str, descriptor: Any) -> Result[CliCommand] {
  match descriptor {
    fields is Record => {
      let declared = command_field_strings(fields, name, "positionals")?
      let declared_rest = optional_text(command_field_string(fields, name, "rest")?)
      let form = optional_text(command_field_string(fields, name, "form")?)
      var positionals = declared
      var rest = declared_rest
      if form != null {
        let parsed = command_form(form ?? "", declared)
        positionals = parsed.positionals
        rest = parsed.rest
      }
      let min_rest = command_field_count(fields, name, "min_rest", 0)?
      let command_like = command_field_bool(fields, name, "command_like", false)?
      let aliases = command_field_strings(fields, name, "aliases")?
      var types: Record = {}
      if fields.has("types") {
        types = command_types(name, fields.get("types") ?? null)?.types
      }
      var options: List[CliOption] = []
      if fields.has("options") {
        let raw_options = fields.get("options") ?? null
        match raw_options {
          option_fields is Record => {
            options = schema_specs(option_fields, "strict")?
          }
          _ => return Err(cli_commands_error(
            f"command `${name}` descriptor field `options` must be Record, found ${type_name(raw_options)}"
          ))
        }
      }
      let spec: CliCommand = {
        canonical: name,
        aliases: aliases,
        positionals: positionals,
        types: types,
        options: options,
        rest: rest,
        min_rest: min_rest,
        command_like: command_like,
      }
      return Ok(spec)
    }
    _ => return Err(cli_commands_error(f"command `${name}` descriptor must be Record"))
  }
}

# Interpret a whole command schema into a lookup keyed by every name a command
# answers to.
#
# A command is looked up under each of its aliases, keyed by the normalized
# spelling, and under its canonical name as written, so a dashed canonical name
# is reachable only through an alias. A repeated alias is a rejection, and the
# canonical name is stored last, where the baseline's `insert` replaces a name an
# earlier command already claimed without reporting it. Commands are visited in
# sorted name order, and their aliases in the order declared, so a schema with
# two collisions reports the baseline's first.
pure command_schema(commands: Record) -> Result[Record] {
  var mapped: Record = {}
  for name in sorted_names(commands.keys()) {
    let spec = command_descriptor(name, commands.get(name) ?? null)?
    for alias in spec.aliases {
      let key = command_key(alias)
      if mapped.has(key) {
        return Err(cli_commands_error(f"duplicate command alias `${alias}`"))
      }
      mapped = record_with_field(mapped, key, spec)
    }
    mapped = record_with_field(mapped, name, spec)
  }
  return Ok(mapped)
}

# The command a name resolves to, or null when no command answers to it.
pure command_named(mapped: Record, key: Str) -> CliCommand? {
  match mapped.get(key) {
    Ok(value) => {
      match value {
        spec is CliCommand => return spec
        _ => return null
      }
    }
    Err(_) => return null
  }
}

# The command a lookup that has already decided it will succeed stands in for.
#
# Like `unrendered_spec`, a looked-up command is only read where the lookup
# itself has already decided the name resolves, so the placeholder is never
# dispatched: it exists to keep the type of the looked-up command known.
pure placeholder_command() -> CliCommand {
  let spec: CliCommand = {
    canonical: "",
    aliases: [],
    positionals: [],
    types: {},
    options: [],
    rest: null,
    min_rest: 0,
    command_like: false,
  }
  return spec
}

# Split one command's arguments into the option walk's argv and its operands.
#
# A command that declares no option takes every argument as an operand, without
# reading `--`, which is the baseline's early return. Otherwise only the names
# the command's options declare are split out: a long name counts when the
# descriptor declares it as a long name or when it names the option itself; a
# short cluster counts when any of its scalars is a declared short name; and
# every other token is an operand. A declared option that takes its value
# separately also consumes the next argument here — even when that argument looks
# like an option — unless it is a flag, already carries `=`, or may omit its
# value.
pure split_command_options(values: List[Str], specs: List[CliOption]) -> Result[CliSplit] {
  if specs.len() == 0 {
    let unsplit: CliSplit = {options: [], positionals: values}
    return Ok(unsplit)
  }
  let long_specs = long_specs_map(specs)?
  let short_specs = short_specs_map(specs)?
  var options: List[Str] = []
  var positionals: List[Str] = []
  var index = 0
  var operands_only = false
  while index < values.len() {
    let token = values.get(index, "")
    if operands_only {
      positionals = positionals.push(token)
      index = index + 1
      continue
    }
    if token == "--" {
      operands_only = true
      index = index + 1
      continue
    }
    if token.starts_with("--") {
      let raw = token.byte_slice(2)
      let parts = raw.split("=", 1)
      let key = normalize_arg_name(parts.get(0, ""))
      var found = false
      var field = ""
      match long_specs.get(key) {
        Ok(value) => {
          match value {
            word is Str => {
              found = true
              field = word
            }
            _ => {}
          }
        }
        Err(_) => {}
      }
      if !found and spec_named(specs, key) != null {
        found = true
        field = key
      }
      if found {
        options = options.push(token)
        let spec = spec_named(specs, field) ?? unrendered_spec()
        if !spec.flag and parts.len() == 1 and !spec.optional_value and index + 1 < values.len() {
          options = options.push(values.get(index + 1, ""))
          index = index + 1
        }
        index = index + 1
        continue
      }
    } else if token.starts_with("-") and token.byte_len() > 1 and !looks_negative_number(token) {
      let cluster = strip_leading_dashes(token)
      var claimed = false
      var offset = 0
      while offset < cluster.byte_len() {
        let step = utf8_step(cluster, offset)
        if short_specs.has(step.scalar) {
          claimed = true
          break
        }
        offset = offset + step.size
      }
      if claimed {
        options = options.push(token)
        index = index + 1
        continue
      }
    }
    positionals = positionals.push(token)
    index = index + 1
  }
  let split: CliSplit = {options: options, positionals: positionals}
  return Ok(split)
}

# Convert one command positional, or reject it.
#
# The baseline's command reader is its option reader with `positional` in place
# of `option`, under the command kind, and reporting the operand's own index in
# the operand list rather than the argument the operand came from. The declared
# types are exactly the six an option can take, so nothing else can reach here.
pure convert_positional_value(name: Str, raw: Str, ty: Str, index: Int) -> Result[CliValue] {
  if ty == "Str" {
    return Ok({value: raw})
  }
  if ty == "Int" {
    let parsed = strict_int(raw)
    if parsed == null {
      return Err(cli_commands_error(
        f"positional `${name}` expects Int at argv[${index}], got `${raw}`"
      ))
    }
    return Ok({value: parsed ?? 0})
  }
  if ty == "UInt" {
    let parsed = strict_int(raw)
    if parsed == null or (parsed ?? 0) < 0 {
      return Err(cli_commands_error(
        f"positional `${name}` expects UInt at argv[${index}], got `${raw}`"
      ))
    }
    return Ok({value: parsed ?? 0})
  }
  if ty == "Bool" {
    let parsed = parse_bool(raw)
    if parsed == null {
      return Err(cli_commands_error(
        f"positional `${name}` expects Bool at argv[${index}], got `${raw}`"
      ))
    }
    return Ok({value: parsed ?? false})
  }
  if ty == "Path" {
    return Ok({value: path_from_text(raw)?})
  }
  let span = duration_from_literal(raw)
  if span == null {
    return Err(cli_commands_error(
      f"positional `${name}` expects Duration at argv[${index}], got `${raw}`"
    ))
  }
  return Ok({value: span ?? time.millis(0)})
}

# One command's parsed record: the command it was dispatched to, the values of
# its option schema, its positionals, and its rest operands.
#
# The options are read by the same walk a schema's options use, over the
# arguments the split collected and with an empty environment, so a command's
# option can take a descriptor default but never an environment fallback; a
# rejection carries no usage text, because the command walk never renders one.
# Option values are merged first and positionals after them, as the baseline
# inserts them, so a positional that repeats an option's name wins. The record is
# rebuilt in sorted name order at the end, which is the order the baseline reads
# its own result back in.
proc command_record(command: Str, values: List[Str], spec: CliCommand) [fs] -> Result[Record] {
  var label = spec.canonical
  if spec.canonical == "fallback_command" {
    label = command
  }
  var output: Record = record_with_field(
    record_with_field({}, "action", command), "command", label
  )
  let split = split_command_options(values, spec.options)?
  if spec.options.len() > 0 {
    let state = parse_values(split.options, spec.options, {}, "strict")?
    for name in state.values.keys() {
      output = record_with_field(output, name, state.values.get(name) ?? null)
    }
  }
  var index = 0
  for name in spec.positionals {
    if index >= split.positionals.len() {
      return Err(cli_commands_error(f"missing positional `${name}` for command `${command}`"))
    }
    let converted = convert_positional_value(
      name, split.positionals.get(index, ""), command_positional_type(spec, name), index
    )?
    output = record_with_field(output, name, converted.value)
    index = index + 1
  }
  let rest_values = [split.positionals.get(i, "") for i in range(index, split.positionals.len())]
  if rest_values.len() < spec.min_rest {
    return Err(cli_commands_error(
      f"command `${command}` expects at least ${spec.min_rest} rest arguments"
    ))
  }
  let rest_name = spec.rest
  if rest_name != null {
    output = record_with_field(output, rest_name ?? "", rest_values)
  } else if index < split.positionals.len() {
    return Err(cli_commands_error(
      f"unexpected positional argument for command `${command}`: ${split.positionals.get(index, "")}"
    ))
  }
  return Ok(sorted_record(output))
}

# Route an argument list to the command that parses it: the baseline's
# `parse_command_values`.
#
# The first token decides. A token that names a command is dispatched with the
# rest of the list; otherwise a fallback command takes the whole list, including
# the token, when it does not ask to be command-like or when the token is
# command-like; otherwise the list is parsed as the rootless default command,
# whose name must name one. A list no command claims reports the token it started
# with, or the missing command when there was none.
proc route_commands(
  argv: List[Str],
  rootless_default: Str,
  mapped: Record,
  fallback: CliCommand?
) [fs] -> Result[Record] {
  if argv.len() > 0 {
    let token = argv.get(0, "")
    let named = command_named(mapped, command_key(token))
    if named != null {
      return command_record(token, argv_tail(argv, 1), named ?? placeholder_command())
    }
    if fallback != null {
      let chosen = fallback ?? placeholder_command()
      if !chosen.command_like or command_like(token) {
        return command_record(token, argv, chosen)
      }
    }
  }
  if rootless_default != "" {
    let named = command_named(mapped, command_key(rootless_default))
    if named == null {
      return Err(cli_commands_error(
        f"unknown rootless default command `${rootless_default}`"
      ))
    }
    return command_record(rootless_default, argv, named ?? placeholder_command())
  }
  if argv.len() > 0 {
    return Err(cli_commands_error(f"unknown command `${argv.get(0, "")}`"))
  }
  return Err(cli_commands_error("missing command"))
}

## Route `argv` to one of the commands a schema declares.
##
## `commands` maps a command name to its descriptor: `positionals` (the
## positional names, in order), `types` (a positional's type, `Str` when
## unlisted), `rest` (the name the extra operands are collected under), `min_rest`
## (how many operands the rest needs), `aliases` (other names the command answers
## to), `form` (a compact spelling of the positionals and the rest),
## `command_like` (whether the command accepts a token that looks like a path),
## and `options` (the command's own option schema, read exactly as `parse` reads
## one).
##
## The result carries `command` (the command's declared name, or the token the
## dispatch used for a fallback command), `action` (the name the argument list
## used, which is an alias when an alias was spelled), one field per option of the
## command's option schema, one field per positional, and the rest operands.
##
## Every rejection is a `cli-commands` error, which carries no usage text: this
## walk renders none.
export proc commands(
  argv: List[Str],
  commands: Record
) [fs] -> Result[Record] {
  return commands_rootless(argv, "", commands, null)
}

## Route `argv` to one of the commands a schema declares, with a default and a
## fallback.
##
## `rootless_default` names the command that parses the whole argument list when
## its first token names no command, with that token as its first positional. It
## must name a command, and an unknown name is the rejection. While it is empty,
## a first token that names no command is the `unknown command` rejection, and an
## empty argument list is the `missing command` rejection.
##
## `fallback_command` is a command descriptor that takes any token that names no
## command, as `commands` describes one. It only takes a token that looks like a
## path when its `command_like` field is set.
export proc commands_rootless(
  argv: List[Str],
  rootless_default: Str,
  commands: Record,
  fallback_command: Record? = null
) [fs] -> Result[Record] {
  let mapped = command_schema(commands)?
  var fallback: CliCommand? = null
  if fallback_command != null {
    fallback = command_descriptor("fallback_command", fallback_command ?? null)?
  }
  return route_commands(argv, rootless_default, mapped, fallback)
}
