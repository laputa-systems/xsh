##! Current schema-aware observer; the pattern row inventory follows its source table.

## A scanner or ratchet failure with source evidence.
export error MetricError = Invalid(path: Str, line: Int, detail: Str) | Rise(name: Str, before: Int, after: Int) | Contract(detail: Str)

type Token = {text: Str, start: Int, end: Int, line: Int, last_line: Int, literal: Bool}
type Lexed = {tokens: List[Token], mates: Map[Int, Int], size: Int, lines: Int}
type Span = {start: Int, end: Int, line: Int, last_line: Int}
type SpanLookupEntry = {start: Int, prefix_end: Int}
type SpanLookup = {entries: List[SpanLookupEntry]}
type Attribute = {start: Int, end: Int, inner: Bool, scope: Int, condition_active: Bool}
type RustModule = {name: Str, directory: Str, explicit_path: Str?, excluded: Bool, dead_code: Bool, line: Int}
type CfgValue = {value: Bool, next: Int}
type CfgPredicate = {start: Int, end: Int}
type InlineModule = {end: Int, directory: Str}
type ModuleEdge = {owner: Str, target: Str, excluded: Bool, dead_code: Bool}
type FunctionRegion = {keyword: Int, name: Str, params_start: Int, params_end: Int, body_start: Int, body_end: Int}
type LegacyPipelineStageLayout = {tag_definitions: Int, tag_variants: List[Str], codec_definitions: Int, codec_tables: Int, codec_variants: List[Str], semantic_definitions: Int}
type AttributeTarget = {kind: Str, name: Str, first: Int, last: Int}
## An outer item allowance kept separate from blanket scopes for review.
export type ItemAllowance = {path: Str, line: Int, last_line: Int, target: Str, kind: Str, covered_by_local_scope: Bool, inherited_module_scope: Bool}
type DeadCodeAnalysis = {blanket: List[Evidence], items: List[ItemAllowance]}
type StructuralScan = {metrics: List[Metric], item_allowances: List[ItemAllowance]}

## One physical Rust file, with exact excluded spans and retained line numbers.
export type SourceCount = {path: Str, physical: Int, production: Int, retained_lines: List[Int], excluded: List[Span]}
## A structural occurrence; each contributes one unit unless its detail states a line scope.
export type Evidence = {path: Str, line: Int, detail: Str}
## A structural metric and the occurrences that determine its value.
export type Metric = {name: Str, value: Int, evidence: List[Evidence]}
## A row requiring measured or reviewed evidence beyond lexical source inventories.
export type Review = {name: Str, requirement: Str}
## A hashed artifact and the method and result that support a review claim.
export type ReviewArtifact = {path: Str, sha256: Str, command: Str, method: Str, result: Str}
## A canonical review row remains present until a reviewer records supporting evidence.
export type ReviewProof = {name: Str, status: Str, reviewer: Str, evidence: List[ReviewArtifact]}
## A tracked product, test, or verifier path captured at the commit used for review.
export type ReviewSource = {path: Str, oid: Str}
## Versioned semantic evidence, separate from the measured source report.
export type ReviewEvidenceBundle = {schema: Int, source_commit: Str, sources: List[ReviewSource], rows: List[ReviewProof]}
## Versioned report; semantic review rows cannot masquerade as measured structural metrics.
export type Report = {schema: Int, sources: List[SourceCount], metrics: List[Metric], reviews: List[Review]}
type MetricValue = {name: Str, value: Int}
type ObserverBaselineProof = {report_path: Str, report_sha256: Str, legacy_report_sha256: Str, successor_report_sha256: Str, metric_values: List[MetricValue], source_count: Int, item_inventory_path: Str, item_inventory_sha256: Str, item_inventory_entries: Int}
## Successor evidence keeps the original observer and report artifacts independently pinned.
export type ObserverSuccessorProvenance = {schema: Int, original_scanner_path: Str, original_scanner_sha256: Str, successor_scanner_path: Str, successor_scanner_sha256: Str, original_baseline_path: Str, original_baseline_sha256: Str, corrected_baseline_path: Str, corrected_baseline_sha256: Str, metadata_sha256: Str, historical_provenance_sha256: Str, baseline_provenance_sha256: Str, historical_ratchet_tests_sha256: Str, baseline_ratchet_tests_sha256: Str, current_ratchet_tests_path: Str, current_ratchet_tests_sha256: Str, start_commit: Str, source_manifest_sha256: Str, same_baseline: ObserverBaselineProof}
## Per-source inventory of narrow dead-code allowances, including inherited module coverage.
export type AllowanceInventory = {schema: Int, entries: List[ItemAllowance]}
## A source measurement and its separate item-allowance inventory.
export type Measurement = {report: Report, item_allowances: AllowanceInventory}
## Proof that the scoped baseline changes only the blanket row and still names the original source tree.
export type BlanketBaselineProvenance = {schema: Int, start_commit: Str, source_manifest_sha256: Str, original_baseline_path: Str, original_baseline_sha256: Str, corrected_baseline_path: Str, corrected_baseline_sha256: Str, corrected_blanket_lines: Int, scanner_sha256: Str, ratchet_tests_sha256: Str}
type BlanketBaselineMetadata = {start_commit: Str, source_manifest_sha256: Str, baseline_report_sha256: Str}
## A lexically validated Rust file with module edges, structural occurrences, and item allowances.
export type SourceScan = {count: SourceCount, modules: List[RustModule], metrics: List[Metric], item_allowances: List[ItemAllowance]}

const REVIEW_REQUIREMENTS: List[Review] = [
  {name: "operand_behavior_differences", requirement: "Verified per-instruction behavior comparison and retained oracle tests; source arm counts cannot establish equivalence."},
  {name: "duplicated_per_item_stages", requirement: "Reviewed stage execution ownership, backed by verified generated stage tables and pipeline differential probes."},
  {name: "nested_machine_bounds", requirement: "Review each entry call and constructor for native-stack nesting and the accepted counter bound; lexical calls alone do not prove scheduling."},
  {name: "instruction_fallback_arms", requirement: "Verified generated instruction dispatch inventory identifying recursive fallback routes."},
  {name: "name_resolution_authorities", requirement: "Reviewed authority boundaries and feature-touch evidence; source definitions do not prove semantic authority."},
  {name: "lint_recursive_descents", requirement: "Verified traversal inventory and rule probes identifying independent recursive descents."},
  {name: "module_checks_per_pass", requirement: "Measured single lint-pass module-check counts with the shared-import fixture."},
  {name: "whole_program_stage_copies", requirement: "Reviewed stage-lowering ownership and allocation probes; arbitrary clone calls are not a semantic count."},
  {name: "open_defects", requirement: "Reviewed TODO defect dispositions with retained regression coverage and recorded reasons."},
  {name: "timings", requirement: "Isolated repository lint and full gate wall-clock measurements; timings are evidence without a rise ratchet."},
]
const PATTERN_SCHEMA_VARIANTS = ["Alias", "Alternation", "List", "TagType", "RecordTest", "ResultTest", "TagTest", "ErrorTest", "Wildcard", "Bind", "Type", "Literal", "ResultOk", "ResultErr", "Facet", "Tag", "Text"]
const PATTERN_SCHEMA_ORDINALS = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "15", "16", "17"]
const REVIEW_SOURCE_PATHS = ["src", "crates", "tests", "core", "examples", "showcase", "tools", "dev/tests", "dev/consolidation_metrics.xsh", "dev/consolidation_metrics_current.xsh", "dev/consolidation/metrics.xsh"]
const SOURCE_ADVISORY_METRICS = ["source_lines_src", "source_lines_xsht"]
const FINAL_ZERO_METRICS = ["manual_payload_calls", "optional_raw_calls", "wide_arena_wildcards", "blanket_dead_code_lines"]
const end_token: Token = Token(text: "", start: 0, end: 0, line: 0, last_line: 0, literal: false)

pure ident_start(value: Str) -> Bool {
  value != "" and (value in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_" or value.byte_len() > 1)
}
pure first(value: Str) -> Str { value.split("").get(0) ?? "" }
pure ident_part(value: Str) -> Bool { ident_start(value) or (value != "" and value in "0123456789") }
proc invalid(source_path: Str, line: Int, detail: Str) [error] { Err(MetricError.Invalid(path: source_path, line:, detail:)) }

# Literal contents never enter the token stream as identifiers or delimiters.
# Offsets are Unicode scalar indices; line ownership is independent of byte width.
proc lex(source_path: Str, source: Str) [error] -> Result[Lexed] {
  let chars = source.split("")
  var byte_offset = 0
  let offsets: List[Int] = collect {
    for scalar in chars { yield byte_offset; byte_offset += scalar.byte_len() }
    yield byte_offset
  }
  var cursor = 0
  var line = 1
  let tokens: List[Token] = collect {
    while cursor < chars.len() {
      let current = chars.get(cursor) ?? ""
      let next = chars.get(cursor + 1) ?? ""
      if current in [" ", "\t", "\r", "\n"] {
        if current == "\n" { line += 1 }
        cursor += 1
        continue
      }
      if current == "/" and next == "/" {
        while cursor < chars.len() and (chars.get(cursor) ?? "") != "\n" { cursor += 1 }
        continue
      }
      if current == "/" and next == "*" {
        let start_line = line
        cursor += 2
        var depth = 1
        while cursor < chars.len() and depth > 0 {
          if (chars.get(cursor) ?? "") == "/" and (chars.get(cursor + 1) ?? "") == "*" { depth += 1; cursor += 2 } else if (chars.get(cursor) ?? "") == "*" and (chars.get(cursor + 1) ?? "") == "/" { depth -= 1; cursor += 2 } else { if (chars.get(cursor) ?? "") == "\n" { line += 1 }; cursor += 1 }
        }
        if depth != 0 { invalid(source_path, start_line, "unterminated block comment") }
        continue
      }
      let start = cursor
      let start_line = line
      if current == "r" and next == "#" and ident_start(chars.get(cursor + 2) ?? "") {
        cursor += 3
        while ident_part(chars.get(cursor) ?? "") { cursor += 1 }
        yield {text: source.byte_slice(offsets[start], length: offsets[cursor] - offsets[start]), start: start, end: cursor, line: start_line, last_line: line, literal: false}
        continue
      }
      var quote = cursor
      var raw = false
      if current in ["b", "c"] and next in ["\"", "'"] { quote += 1 }
      if current == "r" or (current in ["b", "c"] and next == "r") {
        quote = cursor + (if current == "r" { 1 } else { 2 })
        while (chars.get(quote) ?? "") == "#" { quote += 1 }
        if (chars.get(quote) ?? "") == "\"" { raw = true } else { quote = cursor }
      }
      let delimiter = chars.get(quote) ?? ""
      if current == "c" and quote > cursor and delimiter == "'" { invalid(source_path, line, "C character literal is unsupported") }
      if raw and quote - start - (if current == "r" { 1 } else { 2 }) > 255 { invalid(source_path, line, "too many raw literal delimiters") }
      var literal = delimiter == "\"" or (quote > cursor and delimiter == "'")
      if delimiter == "'" and quote == cursor {
        # A lifetime is an apostrophe followed by an identifier without a closing quote.
        if ident_start(next) {
          var end = cursor + 2
          while ident_part(chars.get(end) ?? "") { end += 1 }
          if (chars.get(end) ?? "") != "'" {
            cursor = end
            yield {text: source.byte_slice(offsets[start], length: offsets[cursor] - offsets[start]), start: start, end: cursor, line: start_line, last_line: line, literal: false}
            continue
          }
          if end != cursor + 2 { invalid(source_path, line, "multi-character Rust character literal") }
        }
        literal = true
      }
      if literal {
        cursor = quote + 1
        var closed = false
        var payload = 0
        while cursor < chars.len() {
          let value = chars.get(cursor) ?? ""
          if value == delimiter {
            var end = cursor + 1
            if raw {
              let hashes = quote - start - (if current == "r" { 1 } else { 2 })
              var count = 0
              while count < hashes and (chars.get(end) ?? "") == "#" { count += 1; end += 1 }
              if count != hashes { cursor += 1; continue }
            }
            cursor = end
            closed = true
            break
          }
          if value == "\\" and ! raw {
            cursor += 1
            break when cursor >= chars.len()
            let escape = chars.get(cursor) ?? ""
            if ! (escape in ["\\", "\"", "'", "n", "r", "t", "0", "x", "u", "\n", "\r"]) { invalid(source_path, line, "unsupported Rust escape") }
            if escape == "\n" { line += 1 }
            if delimiter == "'" and escape in ["\n", "\r"] { invalid(source_path, start_line, "newline in character escape") }
            if escape == "u" {
              if current == "b" { invalid(source_path, line, "Unicode escape in byte literal") }
              if (chars.get(cursor + 1) ?? "") != "{" { invalid(source_path, line, "malformed Unicode escape") }
              cursor += 2
              var digits = 0
              var scalar = 0
              while cursor < chars.len() and (chars.get(cursor) ?? "") != "}" {
                let digit = chars.get(cursor) ?? ""
                if digit != "_" {
                  if ! (digit in "0123456789abcdefABCDEF") { invalid(source_path, line, "malformed Unicode escape") }
                  digits += 1
                  scalar = scalar * 16 + ("0123456789abcdef".find(digit.lower()) ?? 0)
                }
                cursor += 1
              }
              if (chars.get(cursor) ?? "") != "}" or digits == 0 or digits > 6 { invalid(source_path, start_line, "malformed Unicode escape") }
              if scalar > 1114111 or (scalar >= 55296 and scalar <= 57343) { invalid(source_path, line, "invalid Unicode scalar escape") }
            } else if escape == "x" {
              for offset in [1, 2] {
                let digit = chars.get(cursor + offset) ?? ""
                if digit == "" or ! (digit in "0123456789abcdefABCDEF") { invalid(source_path, line, "malformed hexadecimal escape") }
              }
              if current != "b" and current != "c" and (chars.get(cursor + 1) ?? "").lower() in "89abcdef" { invalid(source_path, line, "non-ASCII hexadecimal escape") }
              cursor += 2
            }
            cursor += 1
            payload += 1
            continue
          }
          if current == "b" and value.byte_len() > 1 { invalid(source_path, line, "non-ASCII byte literal") }
          if value == "\n" {
            if delimiter == "'" { invalid(source_path, start_line, "newline in character literal") }
            line += 1
          }
          payload += 1
          cursor += 1
        }
        if ! closed { invalid(source_path, start_line, "unterminated Rust literal") }
        if delimiter == "'" and payload != 1 { invalid(source_path, start_line, "invalid character literal") }
      } else {
        cursor += 1
        if ident_start(current) {
          while ident_part(chars.get(cursor) ?? "") { cursor += 1 }
        }
      }
      yield {text: source.byte_slice(offsets[start], length: offsets[cursor] - offsets[start]), start: start, end: cursor, line: start_line, last_line: line, literal: literal}
    }
  }
  var stack = []
  var depth = 0
  var mates: Map[Int, Int] = {}
  var index = 0
  for item in tokens {
    if ! item.literal and item.text in ["(", "[", "{"] {
      if depth < stack.len() { stack[depth] = index } else { stack += [index] }
      depth += 1
    }
    if ! item.literal and item.text in [")", "]", "}"] {
      if depth == 0 { invalid(source_path, item.line, "unmatched closing delimiter") }
      let opening = stack[depth - 1]
      let expected = if item.text == ")" { "(" } else if item.text == "]" { "[" } else { "{" }
      if tokens[opening].text != expected { invalid(source_path, item.line, "mismatched Rust delimiters") }
      mates[opening] = index
      mates[index] = opening
      depth -= 1
    }
    index += 1
  }
  if depth > 0 { invalid(source_path, tokens[stack[0]].line, "unclosed Rust delimiter") }
  let physical = if source == "" { 0 } else { line - (if source.ends_with("\n") { 1 } else { 0 }) }
  {tokens: tokens, mates: mates, size: chars.len(), lines: physical}
}

pure words(lexed: Lexed, start: Int, end: Int) -> Str {
  let values = collect {
    var index = start
    while index < end { yield lexed.tokens[index].text; index += 1 }
  }
  values.join("")
}

proc cfg_eval(source_path: Str, lexed: Lexed, start: Int, assignments: Map[Bool]) [error] -> Result[CfgValue] {
  let name = (lexed.tokens.get(start) ?? end_token).text
  if name in ["all", "any", "not"] and (lexed.tokens.get(start + 1) ?? end_token).text == "(" {
    let end = lexed.mates[start + 1]
    var cursor = start + 2
    var value = name != "any"
    var count = 0
    while cursor < end {
      let child = cfg_eval(source_path, lexed, cursor, assignments)?
      if name == "any" { value = value or child.value } else if name == "all" { value = value and child.value } else { value = ! child.value }
      count += 1
      cursor = child.next
      if cursor < end {
      if (lexed.tokens.get(cursor) ?? end_token).text != "," { invalid(source_path, lexed.tokens[cursor].line, "expected comma in cfg") }
        cursor += 1
      }
    }
      if name == "not" and count != 1 { invalid(source_path, lexed.tokens[start].line, "cfg not requires one predicate") }
    return {value: value, next: end + 1}
  }
      if ! ident_start(first(name)) { invalid(source_path, lexed.tokens[start].line, "unsupported cfg predicate") }
  var end = start + 1
  if (lexed.tokens.get(end) ?? end_token).text == "=" {
          if ! lexed.tokens[end + 1].literal { invalid(source_path, lexed.tokens[start].line, "cfg value must be a literal") }
    end += 2
  }
  let atom = words(lexed, start, end)
  {value: if atom == "test" { false } else { assignments.get(atom) ?? false }, next: end}
}

proc satisfiable(source_path: Str, lexed: Lexed, predicates: List[CfgPredicate], atoms: List[Str], index: Int, assignments: Map[Bool]) [error] -> Result[Bool] {
  guard index >= atoms.len() else {
    return true when satisfiable(source_path, lexed, predicates, atoms, index + 1, assignments.set(atoms[index], false))
    return satisfiable(source_path, lexed, predicates, atoms, index + 1, assignments.set(atoms[index], true))
  }
  for predicate in predicates {
    let value = cfg_eval(source_path, lexed, predicate.start, assignments)?
    if value.next != predicate.end { invalid(source_path, lexed.tokens[predicate.start].line, "trailing cfg tokens") }
    return false when ! value.value
  }
  true
}

proc production_cfg(source_path: Str, lexed: Lexed, predicates: List[CfgPredicate]) [error] -> Result[Bool] {
  var atoms = []
  for predicate in predicates {
    let end = predicate.end
    var cursor = predicate.start
    while cursor < end {
      let name = (lexed.tokens.get(cursor) ?? end_token).text
      if (lexed.tokens.get(cursor + 1) ?? end_token).text == "(" or name in ["(", ")", ","] { cursor += 1; continue }
      var next = cursor + 1
      if (lexed.tokens.get(next) ?? end_token).text == "=" { next += 2 }
      let atom = words(lexed, cursor, next)
      if atom != "test" and ! (atom in atoms) { atoms += [atom] }
      cursor = next
    }
  }
  satisfiable(source_path, lexed, predicates, atoms, 0, {})?
}

proc attributes(source_path: Str, lexed: Lexed) [error] -> Result[List[Attribute]] {
  var cursor = 0
  var scopes = []
  let found: List[Attribute] = collect {
    while cursor < lexed.tokens.len() {
      let name = (lexed.tokens.get(cursor) ?? end_token).text
      if name == "{" { scopes += [cursor] }
      if name == "}" { scopes = scopes |> take(scopes.len() - 1) |> collect }
      if name == "#" {
        let inner = (lexed.tokens.get(cursor + 1) ?? end_token).text == "!"
        let opening = cursor + (if inner { 2 } else { 1 })
        if (lexed.tokens.get(opening) ?? end_token).text != "[" { invalid(source_path, lexed.tokens[cursor].line, "unsupported Rust attribute") }
        let end = lexed.mates[opening]
        let kind = (lexed.tokens.get(opening + 1) ?? end_token).text
        var condition_active = true
        if kind in ["cfg", "cfg_attr"] and ((lexed.tokens.get(opening + 2) ?? end_token).text != "(" or lexed.mates[opening + 2] != end - 1) {
          invalid(source_path, lexed.tokens[cursor].line, "malformed conditional attribute")
        }
        if kind == "cfg_attr" {
          # Conditional metadata is harmless; conditional cfg/source_path or custom
          # attributes could alter item ownership and must be reviewed explicitly.
          var child = opening + 3
          while child < end {
            if (lexed.tokens.get(child) ?? end_token).text in ["(", "[", "{"] { child = lexed.mates[child] + 1; continue }
            break when (lexed.tokens.get(child) ?? end_token).text == ","
            child += 1
          }
          if (lexed.tokens.get(child) ?? end_token).text != "," { invalid(source_path, lexed.tokens[cursor].line, "cfg_attr requires metadata") }
          condition_active = production_cfg(source_path, lexed, [{start: opening + 3, end: child}])?
          child += 1
          while child < end - 1 {
            if ! ((lexed.tokens.get(child) ?? end_token).text in ["allow", "warn", "deny", "forbid", "expect", "derive", "doc"]) {
              invalid(source_path, lexed.tokens[cursor].line, "cfg_attr eligibility is unsupported")
            }
            child += 1
            if (lexed.tokens.get(child) ?? end_token).text == "(" { child = lexed.mates[child] + 1 } else if (lexed.tokens.get(child) ?? end_token).text == "=" { child += 2 } else { invalid(source_path, lexed.tokens[cursor].line, "malformed cfg_attr metadata") }
            if (lexed.tokens.get(child) ?? end_token).text == "," { child += 1 }
          }
        }
        yield {start: cursor, end: end, inner: inner, scope: scopes.get(scopes.len() - 1) ?? -1, condition_active: condition_active}
        cursor = end + 1
      } else { cursor += 1 }
    }
  }
  found
}

proc item_end(source_path: Str, lexed: Lexed, start: Int) [error] -> Result[Int] {
  var cursor = start
  var semicolon = false
  var angles = 0
  var declaration = false
  while cursor < lexed.tokens.len() {
    let name = (lexed.tokens.get(cursor) ?? end_token).text
    # Signature keywords such as const parameters do not change item kind.
    if ! declaration and name in ["fn", "mod", "struct", "enum", "impl", "trait", "union", "extern", "use", "const", "static", "type", "macro_rules"] {
      if name == "const" and (lexed.tokens.get(cursor + 1) ?? end_token).text in ["fn", "unsafe", "async"] { cursor += 1; continue }
      declaration = true
      semicolon = name in ["use", "static", "type", "const"]
    }
    if name == "<" and ! semicolon { angles += 1 }
    if name == ">" and angles > 0 and (lexed.tokens.get(cursor - 1) ?? end_token).text != "-" { angles -= 1 }
    return cursor when name == ";"
    return cursor when name == "," and angles == 0 and ! declaration
    if name == "{" {
      let end = lexed.mates[cursor]
      # A use tree is followed by its semicolon; its braces do not end the item.
      if semicolon or angles > 0 { cursor = end + 1; continue }
      return end + (if (lexed.tokens.get(end + 1) ?? end_token).text in [";", ","] { 1 } else { 0 })
    }
    if name in ["(", "["] { cursor = lexed.mates[cursor] + 1 } else { cursor += 1 }
  }
  invalid(source_path, lexed.tokens[start].line, "cannot determine attributed item boundary")
  start
}

pure span_lookup(spans: List[Span]) -> SpanLookup {
  var prefix_end = -1
  let entries: List[SpanLookupEntry] = collect {
    for span in spans |> sort-by .start {
      prefix_end = if span.end > prefix_end { span.end } else { prefix_end }
      yield {start: span.start, prefix_end: prefix_end}
    }
  }
  {entries: entries}
}

pure inside(item: Token, lookup: SpanLookup) -> Bool {
  var low = 0
  var high = lookup.entries.len()
  while low < high {
    let middle = (low + high) / 2
    if lookup.entries[middle].start <= item.start { low = middle + 1 } else { high = middle }
  }
  return false when low == 0
  lookup.entries[low - 1].prefix_end >= item.end
}

proc scope_start(source_path: Str, lexed: Lexed, opening: Int) [error] -> Result[Int] {
  var start = opening
  var cursor = opening - 1
  var item = false
  while cursor >= 0 {
    let name = (lexed.tokens.get(cursor) ?? end_token).text
    break when name in [";", "{", "}"]
    if name in ["fn", "mod", "impl", "trait", "extern"] { item = true }
    if name in [")", "]"] { cursor = lexed.mates[cursor] }
    start = cursor
    cursor -= 1
  }
  if ! item { invalid(source_path, lexed.tokens[opening].line, "inner cfg scope is not a supported Rust item") }
  start
}

pure struct_body_name(lexed: Lexed, opening: Int) -> Str {
  var cursor = opening - 1
  while cursor >= 0 {
    let value = lexed.tokens[cursor].text
    return lexed.tokens[cursor + 1].text when value == "struct"
    return "" when value in [";", "{", "}"]
    cursor -= 1
  }
  ""
}

proc attributed_item_target(source_path: Str, lexed: Lexed, attr: Attribute) [error] -> Result[AttributeTarget] {
  var cursor = attr.end + 1
  while (lexed.tokens.get(cursor) ?? end_token).text == "#" {
    if (lexed.tokens.get(cursor + 1) ?? end_token).text != "[" { invalid(source_path, lexed.tokens[cursor].line, "unsupported dead_code allowance target") }
    cursor = lexed.mates[cursor + 1] + 1
  }
  while true {
    let value = (lexed.tokens.get(cursor) ?? end_token).text
    if value == "pub" { cursor += 1; if (lexed.tokens.get(cursor) ?? end_token).text == "(" { cursor = lexed.mates[cursor] + 1 }; continue }
    if value in ["unsafe", "async", "default", "auto"] or (value == "const" and (lexed.tokens.get(cursor + 1) ?? end_token).text == "fn") { cursor += 1; continue }
    if value == "extern" {
      cursor += 1
      if (lexed.tokens.get(cursor) ?? end_token).literal { cursor += 1 }
      if (lexed.tokens.get(cursor) ?? end_token).text == "{" {
        let last = item_end(source_path, lexed, attr.end + 1)?
        return {kind: "extern_block", name: "extern", first: lexed.tokens[attr.start].line, last: lexed.tokens[last].line}
      }
      continue
    }
    let record = struct_body_name(lexed, attr.scope)
    if record != "" and (lexed.tokens.get(cursor + 1) ?? end_token).text == ":" {
      let last = item_end(source_path, lexed, attr.end + 1)?
      return {kind: "field", name: f"{record}.{value}", first: lexed.tokens[attr.start].line, last: lexed.tokens[last].line}
    }
    let kind = if value == "mod" { "module" } else if value == "impl" { "impl" } else if value == "trait" { "trait" } else if value in ["fn", "struct", "enum", "union", "type", "const", "static", "use", "extern"] { value } else if value in ["macro_rules", "macro"] and (lexed.tokens.get(cursor + 1) ?? end_token).text == "!" { "macro_definition" } else if ident_start(first(value)) {
      var after = cursor + 1
      while (lexed.tokens.get(after) ?? end_token).text == ":" and (lexed.tokens.get(after + 1) ?? end_token).text == ":" { after += 2; break unless ident_start(first((lexed.tokens.get(after) ?? end_token).text)); after += 1 }
      if (lexed.tokens.get(after) ?? end_token).text == "!" { "macro_invocation" } else { "" }
    } else { "" }
    if kind == "" { invalid(source_path, (lexed.tokens.get(cursor) ?? end_token).line, "unsupported dead_code allowance target") }
    let name = if kind in ["module", "fn", "struct", "enum", "union", "trait", "type", "const", "static", "macro_definition"] { (lexed.tokens.get(cursor + (if kind == "macro_definition" { 2 } else { 1 })) ?? end_token).text } else { kind }
    let last = item_end(source_path, lexed, attr.end + 1)?
    return {kind: kind, name: name, first: lexed.tokens[attr.start].line, last: lexed.tokens[last].line}
  }
  {kind: "", name: "", first: 0, last: 0}
}

pure attribute_allows_dead_code(lexed: Lexed, attr: Attribute) -> Bool {
  let kind = attr.start + (if attr.inner { 3 } else { 2 })
  var cursor = kind
  while cursor < attr.end {
    if lexed.tokens[cursor].text == "allow" and (lexed.tokens.get(cursor + 1) ?? end_token).text == "(" {
      let end = lexed.mates[cursor + 1]
      var argument = cursor + 2
      while argument < end { return true when lexed.tokens[argument].text == "dead_code"; argument += 1 }
      cursor = end + 1
    } else { cursor += 1 }
  }
  false
}

proc dead_code_allowance_analysis(source_path: Str, lexed: Lexed, lookup: SpanLookup, attrs: List[Attribute], count: SourceCount) [error] -> Result[DeadCodeAnalysis] {
  var blanket: List[Evidence] = []
  var scopes: List[Span] = []
  var items: List[ItemAllowance] = []
  for attr in attrs {
    if inside(lexed.tokens[attr.start], lookup) or ! attr.condition_active or ! attribute_allows_dead_code(lexed, attr) { continue }
    var target = AttributeTarget(kind: "", name: "", first: 0, last: 0)
    var is_scope = attr.inner
    var detail = "allow(dead_code) inner scope"
    if attr.inner {
      let scope_first = if attr.scope < 0 { 1 } else { lexed.tokens[scope_start(source_path, lexed, attr.scope)?].line }
      let last = if attr.scope < 0 { lexed.lines } else { lexed.tokens[lexed.mates[attr.scope]].line }
      target = {kind: "inner_scope", name: "scope", first: scope_first, last: last}
    } else {
      target = attributed_item_target(source_path, lexed, attr)?
      is_scope = target.kind in ["module", "impl", "trait", "extern_block"]
      detail = f"allow(dead_code) {target.kind} scope"
    }
    if is_scope {
      scopes += [{start: 0, end: 0, line: target.first, last_line: target.last}]
      for line in count.retained_lines { if line >= target.first and line <= target.last { blanket += [{path: source_path, line: line, detail: detail}] } }
    } else {
      items += [{path: source_path, line: lexed.tokens[attr.start].line, last_line: target.last, target: f"{target.kind}::{target.name}", kind: target.kind, covered_by_local_scope: false, inherited_module_scope: false}]
    }
  }
  let covered = collect {
    for item in items {
      var is_covered = false
      for scope in scopes { if item.line >= scope.line and item.line <= scope.last_line { is_covered = true } }
      yield {...item, covered_by_local_scope: is_covered}
    }
  }
  {blanket: blanket, items: covered}
}

proc excluded_spans(source_path: Str, lexed: Lexed, attrs: List[Attribute]) [error] -> Result[List[Span]] {
  var spans: List[Span] = []
  var cursor = 0
  while cursor < attrs.len() {
    let first_attr = attrs[cursor]
    var last_attr = first_attr
    let predicates: List[CfgPredicate] = collect {
      while true {
        let actual = last_attr.start + (if last_attr.inner { 3 } else { 2 })
        if (lexed.tokens.get(actual) ?? end_token).text == "cfg" {
        if (lexed.tokens.get(actual + 1) ?? end_token).text != "(" { invalid(source_path, lexed.tokens[last_attr.start].line, "malformed cfg attribute") }
          yield {start: actual + 2, end: lexed.mates[actual + 1]}
        }
        cursor += 1
        if cursor >= attrs.len() or attrs[cursor].start != last_attr.end + 1 or attrs[cursor].inner != first_attr.inner { break }
        last_attr = attrs[cursor]
      }
    }
        if ! predicates.is_empty() and ! production_cfg(source_path, lexed, predicates) {
      var start = lexed.tokens[first_attr.start].start
      var end = lexed.size
      var start_line = lexed.tokens[first_attr.start].line
      var last_line = lexed.lines
      if first_attr.inner {
        if first_attr.scope >= 0 {
          let item = scope_start(source_path, lexed, first_attr.scope)?
          start = lexed.tokens[item].start
          start_line = lexed.tokens[item].line
          let closing = lexed.mates[first_attr.scope]
          end = lexed.tokens[closing].end
          last_line = lexed.tokens[closing].last_line
        } else { start = 0; start_line = 1 }
      } else {
        let closing = item_end(source_path, lexed, last_attr.end + 1)?
        end = lexed.tokens[closing].end
        last_line = lexed.tokens[closing].last_line
      }
      spans += [{start: start, end: end, line: start_line, last_line: last_line}]
    }
  }
  spans
}

pure count_lines(source_path: Str, lexed: Lexed, spans: List[Span], lookup: SpanLookup) -> SourceCount {
  var retained: Map[Int, Bool] = {}
  var excluded: Map[Int, Bool] = {}
  var retained_through = 0
  for span in spans {
    var line = span.line
    while line <= span.last_line { excluded[line] = true; line += 1 }
  }
  var span_cursor = 0
  for item in lexed.tokens {
    while span_cursor < lookup.entries.len() and lookup.entries[span_cursor].start <= item.start { span_cursor += 1 }
    let inside_span = span_cursor > 0 and lookup.entries[span_cursor - 1].prefix_end >= item.end
    if ! inside_span {
      var line = if item.line > retained_through { item.line } else { retained_through + 1 }
      while line <= item.last_line { retained[line] = true; line += 1 }
      if item.last_line > retained_through { retained_through = item.last_line }
    }
  }
  let lines: List[Int] = collect {
    var line = 1
    while line <= lexed.lines {
      yield line when ! (excluded.get(line) ?? false) or (retained.get(line) ?? false)
      line += 1
    }
  }
  {path: source_path, physical: lexed.lines, production: lines.len(), retained_lines: lines, excluded: spans}
}

proc modules(source_path: Str, lexed: Lexed, attrs: List[Attribute], lookup: SpanLookup) [error] -> Result[List[RustModule]] {
  let file = fp"{source_path}"
  let stem = file.name().byte_slice(0, length: file.name().byte_len() - 3)
  let directory = if stem in ["lib", "main", "mod"] { file.parent().display() } else { f"{file.parent()}/{stem}" }
  var inline: List[InlineModule] = []
  var cursor = 0
  let edges: List[RustModule] = collect {
    while cursor < lexed.tokens.len() {
        while ! inline.is_empty() and cursor > inline[-1].end { inline = inline |> take(inline.len() - 1) |> collect }
      if (lexed.tokens.get(cursor) ?? end_token).text == "mod" and ident_start(first((lexed.tokens.get(cursor + 1) ?? end_token).text)) {
        let name = (lexed.tokens.get(cursor + 1) ?? end_token).text
          let parent = if inline.is_empty() { directory } else { inline[-1].directory }
          var explicit = null
        var previous = cursor - 1
        if (lexed.tokens.get(previous) ?? end_token).text == ")" { previous = lexed.mates[previous] - 1 }
        if (lexed.tokens.get(previous) ?? end_token).text == "pub" { previous -= 1 }
        let backwards = collect {
          var index = attrs.len() - 1
          while index >= 0 { yield attrs[index]; index -= 1 }
        }
        for attr in backwards {
            guard attr.end == previous else { continue }
          let kind = attr.start + (if attr.inner { 3 } else { 2 })
          if (lexed.tokens.get(kind) ?? end_token).text == "path" {
            let value = (lexed.tokens.get(kind + 2) ?? end_token).text
              if (lexed.tokens.get(kind + 1) ?? end_token).text != "=" { invalid(source_path, lexed.tokens[attr.start].line, "malformed path attribute") }
            if value.starts_with("r") and "\"" in value {
              let parts = value.split("\"", maxsplit: 1)
              let suffix = f"\"{parts[0].byte_slice(1)}"
              explicit = parts[1].byte_slice(0, length: parts[1].byte_len() - suffix.byte_len())
            } else {
              if ! value.starts_with("\"") or ! value.ends_with("\"") or "\\" in value { invalid(source_path, lexed.tokens[attr.start].line, "unsupported path attribute literal") }
              explicit = value.byte_slice(1, length: value.byte_len() - 2)
            }
          }
          previous = attr.start - 1
        }
        # An explicit inline directory replaces the normal module component.
        let base = if inline.is_empty() { file.parent().display() } else { parent }
        if (lexed.tokens.get(cursor + 2) ?? end_token).text == "{" {
          inline += [{end: lexed.mates[cursor + 2], directory: if explicit != null { f"{base}/{explicit}" } else { f"{parent}/{name}" }}]
        }
        if (lexed.tokens.get(cursor + 2) ?? end_token).text == ";" {
          yield {name: name, directory: if explicit != null { base } else { parent }, explicit_path: explicit, excluded: inside(lexed.tokens[cursor], lookup), dead_code: false, line: lexed.tokens[cursor].line}
        }
      }
      cursor += 1
    }
  }
  edges
}

pure metric(name: Str, rows: List[Evidence]) -> Metric { {name: name, value: rows.len(), evidence: rows} }
pure evidence(source_path: Str, item: Token, detail: Str) -> Evidence { {path: source_path, line: item.line, detail: detail} }

# Macro output is a template, but the eager helper is accepted only in the schema macro's generated match arm.
pure token_sequence_matches(lexed: Lexed, opening: Int, expected: List[Str]) -> Bool {
  var offset = 0
  while offset < expected.len() {
    return false when (lexed.tokens.get(opening + offset) ?? end_token).text != expected[offset]
    offset += 1
  }
  true
}

proc validate_instruction_schema_definition(source_path: Str, lexed: Lexed, opening: Int) [error] {
      if source_path != "src/runtime/eval/indexed/full/schema.rs" { invalid(source_path, lexed.tokens[opening].line, "instruction schema eager helper is outside its generated source authority") }
  let closing = lexed.mates[opening]
  let eager_false_arm = ["(", "@", "eager_atom", ")", "=", ">", "{", "false", "}", ";"]
  let eager_true_arm = ["(", "@", "eager_atom", "EagerAtom", ")", "=", ">", "{", "true", "}", ";"]
  let classification_matcher = [
    "$", "effects", ":", "expr", ";", "$", "class", ":", "ident", ";", "$", "strategy", ":", "ident",
    "$", "(", ";", "$", "classification", ":", "ident", ")", "?", "]",
  ]
  let eager_helper_arguments = ["@", "eager_atom", "$", "(", "$", "classification", ")", "?"]
  var false_arms = 0
  var true_arms = 0
  var classification_matchers = 0
  var helper_calls = 0
  var cursor = opening + 1
  while cursor < closing {
    let name = lexed.tokens[cursor].text
    if name == "(" {
      if cursor + eager_false_arm.len() <= closing and token_sequence_matches(lexed, cursor, eager_false_arm) { false_arms += 1 }
      if cursor + eager_true_arm.len() <= closing and token_sequence_matches(lexed, cursor, eager_true_arm) { true_arms += 1 }
    }
    if name == "$" and cursor + classification_matcher.len() <= closing and token_sequence_matches(lexed, cursor, classification_matcher) { classification_matchers += 1 }
    if name == "instruction_schema" and (lexed.tokens.get(cursor + 1) ?? end_token).text == "!" {
      helper_calls += 1
      let call_open = cursor + 2
      if (lexed.tokens.get(call_open) ?? end_token).text != "(" {
      invalid(source_path, lexed.tokens[cursor].line, "malformed instruction schema eager helper invocation")
      }
      let call_close = lexed.mates[call_open]
      let anchor_start = cursor - 7
      let repetition_start = cursor - 11
      let repetition_context = ["$", "(", "$", "("]
      let match_arm_context = ["Self", ":", ":", "$", "tag", "=", ">"]
      if cursor < 11 {
      invalid(source_path, lexed.tokens[cursor].line, "unrecognized instruction schema eager helper expansion")
      }
      let valid_anchor = token_sequence_matches(lexed, repetition_start, repetition_context) and token_sequence_matches(lexed, anchor_start, match_arm_context)
      let valid_arguments = call_close == call_open + eager_helper_arguments.len() + 1 and token_sequence_matches(lexed, call_open + 1, eager_helper_arguments)
      let valid_repetition_end = (lexed.tokens.get(call_close + 1) ?? end_token).text == "," and (lexed.tokens.get(call_close + 2) ?? end_token).text == ")" and (lexed.tokens.get(call_close + 3) ?? end_token).text == "*" and (lexed.tokens.get(call_close + 4) ?? end_token).text == ")" and (lexed.tokens.get(call_close + 5) ?? end_token).text == "*"
      if ! valid_anchor or ! valid_arguments or ! valid_repetition_end {
      invalid(source_path, lexed.tokens[cursor].line, "unrecognized instruction schema eager helper expansion")
      }
    }
    cursor += 1
  }
  if false_arms != 1 or true_arms != 1 or classification_matchers != 1 or helper_calls != 1 {
      invalid(source_path, lexed.tokens[opening].line, "unrecognized instruction schema eager classification definition")
  }
  Ok()
}

# The macro call owns the four instruction tables that generate FullTag.
proc instruction_schema_variants(source_path: Str, lexed: Lexed, opening: Int, strict_classification: Bool) [error] -> Result[List[Evidence]] {
  let closing = lexed.mates[opening]
  let expected_rows = ["BuildIntRow", "BuildBoolRow", "BuildExprRow", "BuildStmtRow"]
    var rows = []
  var variants: List[Evidence] = []
  var tags: Map[Bool] = {}
  var ordinals: Map[Bool] = {}
  var previous_ordinal = -1
  var cursor = opening + 1
  while cursor < closing {
    let row = (lexed.tokens.get(cursor) ?? end_token).text
    let header = if row == "BuildIntRow" { ["IntView", "Int"] } else if row == "BuildBoolRow" { ["BoolView", "Bool"] } else if row == "BuildExprRow" { ["ExprView", "Expr"] } else if row == "BuildStmtRow" { ["StmtView", "Stmt"] } else { [] }
      if header.is_empty() { invalid(source_path, (lexed.tokens.get(cursor) ?? end_token).line, f"unknown instruction schema row {row}") }
    if row in rows { invalid(source_path, (lexed.tokens.get(cursor) ?? end_token).line, f"repeated instruction schema row {row}") }
    rows += [row]
    if (lexed.tokens.get(cursor + 1) ?? end_token).text != "=" or (lexed.tokens.get(cursor + 2) ?? end_token).text != ">" or (lexed.tokens.get(cursor + 3) ?? end_token).text != header[0] or (lexed.tokens.get(cursor + 4) ?? end_token).text != "[" or (lexed.tokens.get(cursor + 5) ?? end_token).text != header[1] or (lexed.tokens.get(cursor + 6) ?? end_token).text != "]" or (lexed.tokens.get(cursor + 7) ?? end_token).text != "{" {
      invalid(source_path, (lexed.tokens.get(cursor) ?? end_token).line, f"malformed instruction schema header for {row}")
    }
    let table_open = cursor + 7
    let table_close = lexed.mates[table_open]
    var entry = table_open + 1
    var entry_count = 0
    var row_variants = []
    while entry < table_close {
      let variant = lexed.tokens.get(entry) ?? end_token
      if variant.text == "" or ! ident_start(first(variant.text)) { invalid(source_path, variant.line, f"malformed instruction schema entry in {row}") }
      if variant.text in row_variants { invalid(source_path, variant.line, f"duplicate instruction schema variant {row}::{variant.text}") }
      row_variants += [variant.text]
      var after_variant = entry + 1
      let fields = (lexed.tokens.get(after_variant) ?? end_token).text
      if fields in ["(", "{"] {
        let fields_close = lexed.mates[after_variant]
        if fields_close <= after_variant or fields_close >= table_close { invalid(source_path, variant.line, f"malformed instruction schema fields for {row}::{variant.text}") }
        after_variant = fields_close + 1
      }
      if (lexed.tokens.get(after_variant) ?? end_token).text != "=" or (lexed.tokens.get(after_variant + 1) ?? end_token).text != ">" {
        invalid(source_path, variant.line, f"malformed instruction schema entry for {row}::{variant.text}")
      }
      let tag = lexed.tokens.get(after_variant + 2) ?? end_token
      if tag.text == "" or ! ident_start(first(tag.text)) or (lexed.tokens.get(after_variant + 3) ?? end_token).text != "=" {
        invalid(source_path, variant.line, f"malformed instruction schema tag for {row}::{variant.text}")
      }
      if tags.get(tag.text) ?? false { invalid(source_path, variant.line, f"duplicate instruction schema tag {tag.text}") }
      tags[tag.text] = true
      var ordinal_end = after_variant + 4
      var ordinal_text = ""
      while (lexed.tokens.get(ordinal_end) ?? end_token).text in "0123456789" {
        ordinal_text += (lexed.tokens.get(ordinal_end) ?? end_token).text
        ordinal_end += 1
      }
      if (lexed.tokens.get(ordinal_end) ?? end_token).text == "-" {
        invalid(source_path, variant.line, f"negative instruction schema ordinal for {row}::{variant.text}")
      }
      if let Ok(ordinal) = ordinal_text.parse_int() {
        let ordinal_key = f"{ordinal}"
        if ordinals.get(ordinal_key) ?? false { invalid(source_path, variant.line, f"duplicate instruction schema ordinal {ordinal_key}") }
        ordinals[ordinal_key] = true
        if ordinal > 255 { invalid(source_path, variant.line, f"instruction schema ordinal {ordinal_key} is outside the u8 range") }
        if ordinal <= previous_ordinal { invalid(source_path, variant.line, f"instruction schema ordinals are not strictly increasing at {ordinal_key}") }
        previous_ordinal = ordinal
      } else { invalid(source_path, variant.line, f"malformed instruction schema ordinal for {row}::{variant.text}") }
      let effects_open = ordinal_end
      if (lexed.tokens.get(effects_open) ?? end_token).text != "[" { invalid(source_path, variant.line, f"malformed instruction schema effects for {row}::{variant.text}") }
      let effects_close = lexed.mates[effects_open]
      if effects_close <= effects_open or effects_close >= table_close or (lexed.tokens.get(effects_close + 1) ?? end_token).text != ";" {
        invalid(source_path, variant.line, f"malformed instruction schema terminator for {row}::{variant.text}: {(lexed.tokens.get(effects_close + 1) ?? end_token).text}")
      }
      if strict_classification {
        var contains_eager_marker = false
        var marker_cursor = effects_open + 1
        let separators = collect {
          while marker_cursor < effects_close {
            let marker = lexed.tokens[marker_cursor]
            if marker.text == "EagerAtom" { contains_eager_marker = true }
            if marker.text in ["(", "[", "{"] { marker_cursor = lexed.mates[marker_cursor] + 1; continue }
              yield marker_cursor when marker.text == ";"
            marker_cursor += 1
          }
        }
        if separators.len() > 3 { invalid(source_path, variant.line, f"malformed instruction schema classification for {row}::{variant.text}") }
        if contains_eager_marker and separators.len() != 3 { invalid(source_path, variant.line, f"malformed instruction schema classification for {row}::{variant.text}") }
        if separators.len() == 3 {
          let classification = lexed.tokens[separators[2] + 1]
          if separators[2] + 2 != effects_close or classification.text != "EagerAtom" or row != "BuildExprRow" {
            invalid(source_path, variant.line, f"unrecognized instruction schema classification for {row}::{variant.text}")
          }
        }
      }
      variants += [evidence(source_path, variant, f"{row}::{variant.text}")]
      entry_count += 1
      entry = effects_close + 2
    }
    if entry_count == 0 { invalid(source_path, (lexed.tokens.get(cursor) ?? end_token).line, f"empty instruction schema row {row}") }
    cursor = table_close + 1
  }
  if (rows |> sort) != (expected_rows |> sort) { invalid(source_path, (lexed.tokens.get(opening) ?? end_token).line, "incomplete instruction schema table set") }
  variants
}

proc legacy_stage_enum_variants(source_path: Str, lexed: Lexed, opening: Int) [error] -> Result[List[Str]] {
  let closing = lexed.mates[opening]
  var cursor = opening + 1
  var variants = []
  while cursor < closing {
    let variant = lexed.tokens[cursor]
    if ! ident_start(first(variant.text)) or variant.text in variants { invalid(source_path, variant.line, "malformed or repeated handwritten FullStageTag variant") }
    variants += [variant.text]
    cursor += 1
    if cursor < closing {
      if lexed.tokens[cursor].text != "," { invalid(source_path, lexed.tokens[cursor].line, "handwritten FullStageTag variants must be unit entries") }
      cursor += 1
    }
  }
  variants
}

proc legacy_stage_codec_variants(source_path: Str, lexed: Lexed, opening: Int) [error] -> Result[List[Str]] {
  let closing = lexed.mates[opening]
  var cursor = opening + 1
  var variants = []
  while cursor < closing {
    if lexed.tokens[cursor].text == "," { cursor += 1; continue }
    let row = lexed.tokens[cursor]
    var paths = []
    var arrows = []
    var member = cursor
    while member < closing and lexed.tokens[member].text != "," {
      let value = lexed.tokens[member].text
      if value in ["(", "[", "{"] { member = lexed.mates[member] + 1; continue }
      if value == "LoweredPipelineStage" and (lexed.tokens.get(member + 1) ?? end_token).text == ":" and (lexed.tokens.get(member + 2) ?? end_token).text == ":" { paths += [(lexed.tokens.get(member + 3) ?? end_token).text] }
      if value == "=" and (lexed.tokens.get(member + 1) ?? end_token).text == ">" { arrows += [member]; member += 2; continue }
      member += 1
    }
    if paths.len() != 2 or paths[0] != paths[1] or arrows.len() != 2 { invalid(source_path, row.line, f"malformed handwritten pipeline stage codec row ({paths.len()} variant paths, {arrows.len()} arrows)") }
    let tag = (lexed.tokens.get(arrows[0] + 2) ?? end_token).text
    let fields = arrows[0] + 3
    if tag != paths[0] or lexed.tokens[fields].text != "{" or lexed.mates[fields] + 1 != arrows[1] {
      invalid(source_path, row.line, "handwritten pipeline stage codec tag or payload does not match its variant")
    }
    variants += [paths[0]]
    cursor = member + 1
  }
  variants
}

proc legacy_pipeline_stage_layout(source_path: Str, lexed: Lexed, lookup: SpanLookup) [error] -> Result[LegacyPipelineStageLayout] {
  var tag_definitions = 0
  var tag_variants = []
  var codec_definitions = 0
  var codec_tables = 0
  var codec_variants = []
  var semantic_definitions = 0
  var cursor = 0
  while cursor < lexed.tokens.len() {
    let item = lexed.tokens[cursor]
    if ! item.literal and ! inside(item, lookup) {
      let next = (lexed.tokens.get(cursor + 1) ?? end_token).text
      if item.text == "enum" and next in ["FullStageTag", "LoweredPipelineStage"] {
        let opening = cursor + 2
        if (lexed.tokens.get(opening) ?? end_token).text != "{" { invalid(source_path, item.line, "malformed handwritten pipeline stage enum") }
        if next == "FullStageTag" { tag_definitions += 1; tag_variants = legacy_stage_enum_variants(source_path, lexed, opening)? } else { semantic_definitions += 1 }
        cursor = lexed.mates[opening] + 1
        continue
      }
      if item.text == "macro_rules" and next == "!" and (lexed.tokens.get(cursor + 2) ?? end_token).text == "impl_stage_codec" {
        codec_definitions += 1
        let opening = cursor + 3
        if (lexed.tokens.get(opening) ?? end_token).text != "{" { invalid(source_path, item.line, "malformed handwritten pipeline stage codec definition") }
        let closing = lexed.mates[opening]
        for required in ["FullCodec", "LoweredPipelineStage", "FullStageTag", "encode", "decode", "verify"] {
          if ! contains_token(lexed, opening + 1, closing, required) { invalid(source_path, item.line, "incomplete handwritten pipeline stage codec definition") }
        }
        cursor = closing + 1
        continue
      }
      if item.text == "impl_stage_codec" and next == "!" {
        codec_tables += 1
        let opening = cursor + 2
        if (lexed.tokens.get(opening) ?? end_token).text != "{" { invalid(source_path, item.line, "handwritten pipeline stage codec table must use a block") }
        codec_variants = legacy_stage_codec_variants(source_path, lexed, opening)?
        cursor = lexed.mates[opening] + 1
        continue
      }
    }
    if item.text in ["(", "[", "{"] { cursor = lexed.mates[cursor] + 1 } else { cursor += 1 }
  }
  {tag_definitions: tag_definitions, tag_variants: tag_variants, codec_definitions: codec_definitions, codec_tables: codec_tables, codec_variants: codec_variants, semantic_definitions: semantic_definitions}
}

# The generated lowered-stage enum has no source members; its schema invocation owns them.
proc pipeline_stage_schema_variants(source_path: Str, lexed: Lexed, opening: Int) [error] -> Result[List[Evidence]] {
  let closing = lexed.mates[opening]
  var cursor = opening + 1
  if (lexed.tokens.get(cursor) ?? end_token).text != "@" or (lexed.tokens.get(cursor + 1) ?? end_token).text != "context" {
    invalid(source_path, (lexed.tokens.get(opening) ?? end_token).line, "malformed pipeline stage schema context")
  }
  cursor += 2
  var context = []
  while cursor < closing and lexed.tokens[cursor].text != ";" { context += [lexed.tokens[cursor].text]; cursor += 1 }
  let core_context = ["stage_builder", "stage_decoder", "stage_payload"]
  let view_context = [@core_context, "stage_view_context", "stage_view_input", "stage_view_value", "stage_view_expectations"]
  if (context != core_context and context != view_context) or (lexed.tokens.get(cursor) ?? end_token).text != ";" {
    invalid(source_path, (lexed.tokens.get(opening) ?? end_token).line, "malformed pipeline stage schema context")
  }
  cursor += 1

  var seen: Map[Bool] = {}
  let variants: List[Evidence] = collect {
    while cursor < closing {
      let variant = lexed.tokens.get(cursor) ?? end_token
      if variant.text == "_" or ! ident_start(first(variant.text)) {
        invalid(source_path, variant.line, "malformed pipeline stage schema entry")
      }
      if seen.get(variant.text) ?? false {
        invalid(source_path, variant.line, f"repeated pipeline stage schema variant {variant.text}")
      }
      seen[variant.text] = true
      let form = (lexed.tokens.get(cursor + 1) ?? end_token).text
      if form not in ["unit", "tuple", "struct"] {
        invalid(source_path, variant.line, f"unknown pipeline stage schema form {form}")
      }
      let fields_open = cursor + 2
      if (lexed.tokens.get(fields_open) ?? end_token).text != "{" {
        invalid(source_path, variant.line, f"malformed pipeline stage schema fields for {variant.text}")
      }
      let fields_close = lexed.mates[fields_open]
      if fields_close <= fields_open or fields_close >= closing {
        invalid(source_path, variant.line, f"malformed pipeline stage schema fields for {variant.text}")
      }
      if form == "unit" {
        if fields_open + 1 != fields_close {
          invalid(source_path, variant.line, f"unit pipeline stage has fields in {variant.text}")
        }
      } else if fields_open + 1 == fields_close {
        invalid(source_path, variant.line, f"empty pipeline stage schema fields in {variant.text}")
      }
      if (lexed.tokens.get(fields_close + 1) ?? end_token).text != ";" {
        invalid(source_path, variant.line, f"malformed pipeline stage schema terminator for {variant.text}")
      }
      yield evidence(source_path, variant, f"LoweredPipelineStage::{variant.text}")
      cursor = fields_close + 2
    }
  }
  if variants.is_empty() {
    invalid(source_path, (lexed.tokens.get(opening) ?? end_token).line, "empty pipeline stage schema")
  }
  variants
}

pure contains_token(lexed: Lexed, start: Int, end: Int, value: Str) -> Bool {
  var cursor = start
  while cursor < end {
    let token = lexed.tokens[cursor]
    return true when ! token.literal and token.text == value
    cursor += 1
  }
  false
}

proc function_regions_for_parameters(source_path: Str, lexed: Lexed, lookup: SpanLookup, required_parameters: List[Str]) [error] -> Result[List[FunctionRegion]] {
  let regions: List[FunctionRegion] = collect {
    var keyword = 0
    while keyword < lexed.tokens.len() {
      let item = lexed.tokens[keyword]
      if ! item.literal and item.text == "fn" and ! inside(item, lookup) {
        let name = (lexed.tokens.get(keyword + 1) ?? end_token).text
        if ident_start(first(name)) {
          var params_start = keyword + 2
          var generic_depth = 0
          while params_start < lexed.tokens.len() {
            let value = lexed.tokens[params_start].text
            if value == "<" { generic_depth += 1; params_start += 1; continue }
            if value == ">" and (lexed.tokens.get(params_start - 1) ?? end_token).text != "-" and generic_depth > 0 {
              generic_depth -= 1
              params_start += 1
              continue
            }
            break when value == "(" and generic_depth == 0
            break when value in ["{", ";"] and generic_depth == 0
            if value in ["(", "[", "{"] { params_start = lexed.mates[params_start] + 1 } else { params_start += 1 }
          }
          if params_start < lexed.tokens.len() and lexed.tokens[params_start].text == "(" {
            let params_end = lexed.mates[params_start]
            var has_required_parameter = false
            var parameter = params_start + 1
            while parameter < params_end {
              if ! lexed.tokens[parameter].literal and lexed.tokens[parameter].text in required_parameters {
                has_required_parameter = true
                break
              }
              parameter += 1
            }
            if has_required_parameter {
              var body_start = params_end + 1
              var angle_depth = 0
              var declaration = false
              while body_start < lexed.tokens.len() {
                let value = lexed.tokens[body_start].text
                if value == "<" { angle_depth += 1; body_start += 1; continue }
                if value == ">" and (lexed.tokens.get(body_start - 1) ?? end_token).text != "-" and angle_depth > 0 {
                  angle_depth -= 1
                  body_start += 1
                  continue
                }
                if value == ";" and angle_depth == 0 { declaration = true; break }
                if value == "{" {
                  break when angle_depth == 0
                  body_start = lexed.mates[body_start] + 1
                  continue
                }
                if value in ["(", "["] { body_start = lexed.mates[body_start] + 1 } else { body_start += 1 }
              }
              if ! declaration {
                if body_start >= lexed.tokens.len() or lexed.tokens[body_start].text != "{" {
                  invalid(source_path, item.line, "cannot locate conversion function body")
                }
                yield {keyword: keyword, name: name, params_start: params_start + 1, params_end: params_end, body_start: body_start + 1, body_end: lexed.mates[body_start]}
              }
            }
          }
        }
      }
      keyword += 1
    }
  }
  regions
}

proc type_storage_mapping_evidence(source_path: Str, lexed: Lexed, lookup: SpanLookup) [error] -> Result[List[Evidence]] {
  return [] when ! source_path.starts_with("src/runtime/eval/")
  var has_type = false
  var has_lowered_type = false
  var has_match = false
  for token in lexed.tokens {
    if ! token.literal {
      if token.text == "Type" { has_type = true }
      if token.text == "LoweredType" { has_lowered_type = true }
      if token.text == "match" { has_match = true }
      break when has_type and has_lowered_type and has_match
    }
  }
  return [] when ! has_type or ! has_lowered_type or ! has_match
  let regions = function_regions_for_parameters(source_path, lexed, lookup, ["Type", "LoweredType"])?
  let found: List[Evidence] = collect {
    for region in regions {
      let item = lexed.tokens[region.keyword]
      var takes_type = false
      var takes_lowered_type = false
      var parameter = region.params_start
      while parameter < region.params_end {
        let token = lexed.tokens[parameter]
        if ! token.literal and token.text == "Type" { takes_type = true }
        if ! token.literal and token.text == "LoweredType" { takes_lowered_type = true }
        parameter += 1
      }
      if takes_type != takes_lowered_type {
        let return_type = if takes_type { "LoweredType" } else { "Type" }
        let signature_end = region.body_start - 1
        var cursor = region.params_end + 1
        var has_return_type = false
        while cursor + 1 < signature_end {
          if lexed.tokens[cursor].text == "-" and lexed.tokens[cursor + 1].text == ">" {
            cursor += 2
            while cursor < signature_end {
              let token = lexed.tokens[cursor]
              if ! token.literal and token.text == return_type { has_return_type = true }
              cursor += 1
            }
            break
          }
          cursor += 1
        }
        if has_return_type {
          var body_has_match = false
          var type_variants = []
          var lowered_type_variants = []
          var body = region.body_start
          while body < region.body_end {
            let token = lexed.tokens[body]
            let value = token.text
            if ! token.literal and value == "match" { body_has_match = true }
            if body + 2 < region.body_end and value in ["Type", "LoweredType"] and lexed.tokens[body + 1].text == ":" and lexed.tokens[body + 2].text == ":" {
              let variant = (lexed.tokens.get(body + 3) ?? end_token).text
              if variant != "" {
                if value == "Type" and ! (variant in type_variants) { type_variants += [variant] }
                if value == "LoweredType" and ! (variant in lowered_type_variants) { lowered_type_variants += [variant] }
              }
            }
            body += 1
          }
          if body_has_match {
            let source_variants = if takes_type { type_variants.len() } else { lowered_type_variants.len() }
            let target_variants = if takes_type { lowered_type_variants.len() } else { type_variants.len() }
            if source_variants >= 8 and target_variants >= 8 {
              yield evidence(source_path, item, region.name)
            } else if source_variants >= 8 or target_variants >= 8 {
              invalid(source_path, item.line, "unclassified Type/LoweredType conversion body")
            }
          }
        }
      }
    }
  }
  found
}

# Pattern rows are generated from one table whose ordinal hole preserves the wire tags.
proc pattern_schema_variants(source_path: Str, lexed: Lexed, opening: Int) [error] -> Result[List[Evidence]] {
  let closing = lexed.mates[opening]
  var cursor = opening + 1
  if (lexed.tokens.get(cursor) ?? end_token).text != "enum" or (lexed.tokens.get(cursor + 1) ?? end_token).text != "FullPatternTag" or (lexed.tokens.get(cursor + 2) ?? end_token).text != "{" {
    invalid(source_path, (lexed.tokens.get(opening) ?? end_token).line, "unrecognized pattern schema enum")
  }
  let rows_open = cursor + 2
  let rows_close = lexed.mates[rows_open]
  if rows_close <= rows_open or rows_close >= closing or rows_close + 1 != closing {
    invalid(source_path, (lexed.tokens.get(opening) ?? end_token).line, "unrecognized pattern schema table boundary")
  }
  cursor = rows_open + 1
  var names = []
  var ordinals = []
  var seen_names: Map[Bool] = {}
  var seen_ordinals: Map[Bool] = {}
  var found: List[Evidence] = []
  while cursor < rows_close {
    while (lexed.tokens.get(cursor) ?? end_token).text == "#" {
      if (lexed.tokens.get(cursor + 1) ?? end_token).text != "[" {
        invalid(source_path, lexed.tokens[cursor].line, "malformed pattern schema entry attribute")
      }
      cursor = lexed.mates[cursor + 1] + 1
    }
    break when cursor == rows_close
    let variant = lexed.tokens.get(cursor) ?? end_token
    if variant.text == "_" or ! ident_start(first(variant.text)) {
      invalid(source_path, variant.line, "malformed pattern schema entry")
    }
    if seen_names.get(variant.text) ?? false {
      invalid(source_path, variant.line, f"duplicate pattern schema variant {variant.text}")
    }
    let equals = lexed.tokens.get(cursor + 1) ?? end_token
    let ordinal = lexed.tokens.get(cursor + 2) ?? end_token
    var ordinal_text = ""
    var ordinal_cursor = cursor + 2
    var ordinal_end = ordinal.start
    while ordinal_cursor < rows_close and lexed.tokens[ordinal_cursor].text in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"] and (ordinal_text == "" or lexed.tokens[ordinal_cursor].start == ordinal_end) {
      ordinal_text = f"{ordinal_text}{lexed.tokens[ordinal_cursor].text}"
      ordinal_end = lexed.tokens[ordinal_cursor].end
      ordinal_cursor += 1
    }
    if equals.text != "=" or !(ordinal_text in PATTERN_SCHEMA_ORDINALS) {
      invalid(source_path, variant.line, "malformed pattern schema entry")
    }
    if seen_ordinals.get(ordinal_text) ?? false {
      invalid(source_path, ordinal.line, f"duplicate pattern schema ordinal {ordinal_text} at {variant.text}")
    }
    seen_names[variant.text] = true
    seen_ordinals[ordinal_text] = true
    names += [variant.text]
    ordinals += [ordinal_text]
    cursor = ordinal_cursor
    let payload = (lexed.tokens.get(cursor) ?? end_token).text
    if payload in ["{", "("] { cursor = lexed.mates[cursor] + 1 }
    if (lexed.tokens.get(cursor) ?? end_token).text != "[" {
      invalid(source_path, variant.line, "malformed pattern schema entry")
    }
    let mode = lexed.tokens.get(cursor + 1) ?? end_token
    let comma = lexed.tokens.get(cursor + 2) ?? end_token
    let constraint = lexed.tokens.get(cursor + 3) ?? end_token
    let bracket = lexed.tokens.get(cursor + 4) ?? end_token
    if !(mode.text in ["Normal", "Text"]) or comma.text != "," or !(constraint.text in ["None", "CaptureSlots", "ListRest", "Alternation"]) or bracket.text != "]" {
      invalid(source_path, variant.line, "malformed pattern schema entry")
    }
    cursor += 5
    found += [evidence(source_path, variant, f"pattern_schema::{variant.text}")]
    let separator = (lexed.tokens.get(cursor) ?? end_token).text
    if separator == "," { cursor += 1 } else if cursor != rows_close { invalid(source_path, variant.line, "malformed pattern schema entry separator") }
  }
  if names != PATTERN_SCHEMA_VARIANTS or ordinals != PATTERN_SCHEMA_ORDINALS {
    invalid(source_path, (lexed.tokens.get(opening) ?? end_token).line, "unrecognized pattern schema inventory")
  }
  found
}

# Macro output is a template, not a second handwritten source authority.
pure inside_macro_definition(lexed: Lexed, index: Int) -> Bool {
  var cursor = 0
  while cursor < index {
    if lexed.tokens[cursor].text == "macro_rules" and (lexed.tokens.get(cursor + 1) ?? end_token).text == "!" {
      let opening = cursor + 3
      if opening < index and (lexed.tokens.get(opening) ?? end_token).text == "{" and lexed.mates[opening] > index { return true }
    }
    cursor += 1
  }
  false
}

proc lowering_type_resolver_evidence(source_path: Str, lexed: Lexed, lookup: SpanLookup) [error] -> Result[List[Evidence]] {
  if source_path != "src/runtime/eval/lower.rs" and ! source_path.starts_with("src/runtime/eval/lower/") { return [] }
  let regions = function_regions_for_parameters(source_path, lexed, lookup, ["TypeExprId"])?
  let found: List[Evidence] = collect {
    for region in regions {
      let item = lexed.tokens[region.keyword]
      var type_dispatch = false
      var recursive = false
      var constructs_type = false
      var body = region.body_start
      while body < region.body_end {
        let token = lexed.tokens[body]
        let value = token.text
        if ! token.literal and value == "ArenaTypeExprTag" { type_dispatch = true }
        if value == region.name and (lexed.tokens.get(body + 1) ?? end_token).text == "(" { recursive = true }
        if body + 2 < region.body_end and value in ["Type", "LoweredType"] and lexed.tokens[body + 1].text == ":" and lexed.tokens[body + 2].text == ":" {
          constructs_type = true
        }
        body += 1
      }
      if type_dispatch {
        var returns_string = false
        var returns_type = false
        var returns_lowered_type = false
        let signature_end = region.body_start - 1
        var signature = region.params_end + 1
        while signature + 1 < signature_end {
          if lexed.tokens[signature].text == "-" and lexed.tokens[signature + 1].text == ">" {
            signature += 2
            while signature < signature_end {
              let token = lexed.tokens[signature]
              if ! token.literal {
                if token.text == "String" { returns_string = true }
                if token.text == "Type" { returns_type = true }
                if token.text == "LoweredType" { returns_lowered_type = true }
              }
              signature += 1
            }
            break
          }
          signature += 1
        }
        if ! returns_string {
          if constructs_type {
            yield evidence(source_path, item, region.name)
          } else if returns_type or returns_lowered_type or recursive {
            invalid(source_path, item.line, "unclassified type-expression tag walker")
          }
        }
      }
    }
  }
  found
}

proc structural(source_path: Str, lexed: Lexed, lookup: SpanLookup, attrs: List[Attribute], count: SourceCount) [error] -> Result[StructuralScan] {
  let families = ["ArenaExprKind", "ArenaStmtKind", "ArenaPatternKind"]
  let legacy_stages = legacy_pipeline_stage_layout(source_path, lexed, lookup)?
  var aliases: Map[Str] = {}
  var calls: List[Evidence] = []
  var optional: List[Evidence] = []
  var instructions: List[Evidence] = []
  var patterns: List[Evidence] = []
  var stages: List[Evidence] = []
  var wildcards: List[Evidence] = []
  var predicates: List[Evidence] = []
  let mappings = type_storage_mapping_evidence(source_path, lexed, lookup)?
  let resolvers = lowering_type_resolver_evidence(source_path, lexed, lookup)?
  var factories: List[Evidence] = []
  var instruction_schema_calls = 0
  var instruction_schema_definition = false
  var instruction_schema_definitions = 0
  var instruction_schema_definition_opening = -1
  var hand_instruction_rows = false
  var pattern_schema_calls = 0
  var pattern_schema_definitions = 0
  var hand_pattern_rows = false
  var pipeline_stage_schema_calls = 0
  var pipeline_stage_schema_definitions = 0
  var pipeline_stage_schema_definition = false
  var cursor = 0
  var span_cursor = 0
  while cursor < lexed.tokens.len() {
    let item = lexed.tokens[cursor]
    let name = item.text
    while span_cursor < lookup.entries.len() and lookup.entries[span_cursor].start <= item.start { span_cursor += 1 }
    let inside_span = span_cursor > 0 and lookup.entries[span_cursor - 1].prefix_end >= item.end
    if ! item.literal and ! inside_span {
      if name == "macro_rules" and (lexed.tokens.get(cursor + 1) ?? end_token).text == "!" and (lexed.tokens.get(cursor + 2) ?? end_token).text == "pipeline_stage_schema" {
        pipeline_stage_schema_definitions += 1
        pipeline_stage_schema_definition = true
        cursor = lexed.mates[cursor + 3] + 1
        continue
      }
      if name == "macro_rules" and (lexed.tokens.get(cursor + 1) ?? end_token).text == "!" and (lexed.tokens.get(cursor + 2) ?? end_token).text == "instruction_schema" {
        instruction_schema_definitions += 1
        if instruction_schema_definitions > 1 { invalid(source_path, item.line, "multiple instruction schema definitions in one source file") }
        let opening = cursor + 3
        if (lexed.tokens.get(opening) ?? end_token).text != "{" { invalid(source_path, item.line, "malformed instruction schema definition") }
        instruction_schema_definition = true
        instruction_schema_definition_opening = opening
        cursor = lexed.mates[opening] + 1
        continue
      }
      if name == "macro_rules" and (lexed.tokens.get(cursor + 1) ?? end_token).text == "!" and (lexed.tokens.get(cursor + 2) ?? end_token).text == "pattern_schema" { pattern_schema_definitions += 1 }
      if name == "instruction_schema" and (lexed.tokens.get(cursor + 1) ?? end_token).text == "!" {
        if inside_macro_definition(lexed, cursor) { invalid(source_path, item.line, "instruction schema invocation is nested in an unrelated macro definition") }
        let opening = cursor + 2
        if (lexed.tokens.get(opening) ?? end_token).text != "{" { invalid(source_path, item.line, "instruction schema invocation must use a table block") }
        instruction_schema_calls += 1
        instructions += instruction_schema_variants(source_path, lexed, opening, instruction_schema_definition)?
      }
      if name == "pattern_schema" and (lexed.tokens.get(cursor + 1) ?? end_token).text == "!" {
        let opening = cursor + 2
        if (lexed.tokens.get(opening) ?? end_token).text != "{" { invalid(source_path, item.line, "pattern schema invocation must use a table block") }
        pattern_schema_calls += 1
        patterns += pattern_schema_variants(source_path, lexed, opening)?
      }
      if name == "pipeline_stage_schema" and (lexed.tokens.get(cursor + 1) ?? end_token).text == "!" {
        let opening = cursor + 2
        if (lexed.tokens.get(opening) ?? end_token).text != "{" { invalid(source_path, item.line, "pipeline stage schema invocation must use a table block") }
        pipeline_stage_schema_calls += 1
        stages += pipeline_stage_schema_variants(source_path, lexed, opening)?
      }
      if name in families and (lexed.tokens.get(cursor + 1) ?? end_token).text == "as" { aliases[(lexed.tokens.get(cursor + 2) ?? end_token).text] = name }
      if name in ["indexed_raw", "indexed_decode", "indexed_finish", "indexed_optional_raw"] and (lexed.tokens.get(cursor - 1) ?? end_token).text != "fn" {
        let next = (lexed.tokens.get(cursor + 1) ?? end_token).text
        if next == "(" or (next == ":" and (lexed.tokens.get(cursor + 2) ?? end_token).text == ":" and (lexed.tokens.get(cursor + 3) ?? end_token).text == "<") {
          if name == "indexed_optional_raw" { optional += [evidence(source_path, item, name)] } else { calls += [evidence(source_path, item, name)] }
        }
      }
      if name == "fn" {
        let definition = (lexed.tokens.get(cursor + 1) ?? end_token).text
        if definition in ["value_matches_static_type", "lowered_value_matches_static_type", "test_value_matches_type"] { predicates += [evidence(source_path, item, definition)] }
      }
      if name == "enum" and (lexed.tokens.get(cursor + 2) ?? end_token).text == "{" {
        let definition = (lexed.tokens.get(cursor + 1) ?? end_token).text
        var include_pattern_enum = true
        if definition == "BuildPatternRow" {
          include_pattern_enum = !inside_macro_definition(lexed, cursor)
          if include_pattern_enum { hand_pattern_rows = true }
        }
        if definition in ["BuildIntRow", "BuildBoolRow", "BuildExprRow", "BuildStmtRow", "LoweredPipelineStage"] or (definition == "BuildPatternRow" and include_pattern_enum) {
          if definition in ["BuildIntRow", "BuildBoolRow", "BuildExprRow", "BuildStmtRow"] { hand_instruction_rows = true }
          let closing = lexed.mates[cursor + 2]
          var member = cursor + 3
          var at_start = true
          while member < closing {
            let value = (lexed.tokens.get(member) ?? end_token).text
            if value == "#" { member = lexed.mates[member + 1] + 1; continue }
            if at_start and ident_start(first(value)) and ! inside(lexed.tokens[member], lookup) {
              let found = evidence(source_path, lexed.tokens[member], f"{definition}::{value}")
              if definition == "BuildPatternRow" { patterns += [found] } else if definition == "LoweredPipelineStage" { stages += [found] } else { instructions += [found] }
              at_start = false
            }
            if value in ["(", "[", "{"] { member = lexed.mates[member] + 1; continue }
            if value == "," { at_start = true }
            member += 1
          }
        }
      }
      if name == "match" {
        var opening = cursor + 1
        while opening < lexed.tokens.len() and (lexed.tokens.get(opening) ?? end_token).text != "{" {
          if (lexed.tokens.get(opening) ?? end_token).text in ["(", "["] { opening = lexed.mates[opening] + 1 } else { opening += 1 }
        }
        if opening >= lexed.tokens.len() { invalid(source_path, item.line, "match body not found") }
        let closing = lexed.mates[opening]
        var member = opening + 1
        var in_pattern = true
          var names = []
        var wildcard = false
        var last_wildcard = false
        while member < closing {
          let value = (lexed.tokens.get(member) ?? end_token).text
          if value == "=" and (lexed.tokens.get(member + 1) ?? end_token).text == ">" {
            last_wildcard = wildcard
            wildcard = false
            in_pattern = false
            member += 2
            continue
          }
          if in_pattern {
            if value == "_" and (lexed.tokens.get(member + 1) ?? end_token).text == "=" and (lexed.tokens.get(member + 2) ?? end_token).text == ">" { wildcard = true }
            let family = aliases.get(value) ?? value
            if family in families and (lexed.tokens.get(member + 1) ?? end_token).text == ":" and (lexed.tokens.get(member + 2) ?? end_token).text == ":" {
              let variant = f"{family}::{(lexed.tokens.get(member + 3) ?? end_token).text}"
              if ! (variant in names) { names += [variant] }
            }
          }
          if value in ["(", "[", "{"] {
            let end = lexed.mates[member]
            if in_pattern {
              var nested = member + 1
              while nested < end {
                let head = lexed.tokens[nested].text
                let family = aliases.get(head) ?? head
                if family in families and (lexed.tokens.get(nested + 1) ?? end_token).text == ":" and (lexed.tokens.get(nested + 2) ?? end_token).text == ":" {
                  let variant = f"{family}::{lexed.tokens[nested + 3].text}"
                  if ! (variant in names) { names += [variant] }
                }
                nested += 1
              }
            }
            member = end + 1
            if ! in_pattern and value == "{" { in_pattern = true }
            continue
          }
          if value == "," { in_pattern = true }
          member += 1
        }
        if last_wildcard and names.len() >= 8 { wildcards += [evidence(source_path, item, names.join(","))] }
      }
      if name == "ExplicitFrames" and (lexed.tokens.get(cursor + 1) ?? end_token).text == ":" and (lexed.tokens.get(cursor + 2) ?? end_token).text == ":" and (lexed.tokens.get(cursor + 3) ?? end_token).text == "new" and (lexed.tokens.get(cursor + 4) ?? end_token).text == "(" {
        factories += [evidence(source_path, item, name)]
      }
    }
    cursor += 1
  }
  if instruction_schema_calls > 1 { invalid(source_path, 1, "multiple instruction schema tables in one source file") }
  if instruction_schema_definition and instruction_schema_calls != 1 { invalid(source_path, 1, "instruction schema definition has no supported table invocation") }
  if source_path == "src/runtime/eval/indexed/full/schema.rs" and instruction_schema_calls != 1 { invalid(source_path, 1, "unrecognized generated instruction schema layout") }
  if instruction_schema_definition_opening >= 0 { validate_instruction_schema_definition(source_path, lexed, instruction_schema_definition_opening) }
  if instruction_schema_calls > 0 and hand_instruction_rows { invalid(source_path, 1, "handwritten instruction rows overlap the generated schema") }
  if pattern_schema_definitions > 1 { invalid(source_path, 1, "multiple pattern schema definitions in one source file") }
  if pattern_schema_calls > 1 { invalid(source_path, 1, "multiple pattern schema tables in one source file") }
  if pattern_schema_definitions > 0 and pattern_schema_calls != 1 { invalid(source_path, 1, "pattern schema definition has no supported table invocation") }
  if pattern_schema_calls > 0 and hand_pattern_rows { invalid(source_path, 1, "pattern schema overlaps the handwritten BuildPatternRow enum") }
  if pipeline_stage_schema_definitions > 1 { invalid(source_path, 1, "multiple pipeline stage schema definitions in one source file") }
  if pipeline_stage_schema_calls > 1 { invalid(source_path, 1, "multiple pipeline stage schema tables in one source file") }
  if pipeline_stage_schema_definition and pipeline_stage_schema_calls != 1 { invalid(source_path, 1, "pipeline stage schema definition has no supported table invocation") }
  let has_legacy_stage_layout = legacy_stages.tag_definitions > 0 or legacy_stages.codec_definitions > 0 or legacy_stages.codec_tables > 0 or legacy_stages.semantic_definitions > 0
  let has_generated_stage_layout = pipeline_stage_schema_definitions > 0 or pipeline_stage_schema_calls > 0
  if has_generated_stage_layout and has_legacy_stage_layout { invalid(source_path, 1, "pipeline stage schema and handwritten pipeline stage layouts overlap") }
  if source_path == "src/runtime/eval/indexed/full.rs" {
    if pipeline_stage_schema_calls > 0 {
      if pipeline_stage_schema_definitions != 1 or pipeline_stage_schema_calls != 1 { invalid(source_path, 1, "unrecognized generated pipeline stage schema layout") }
    } else if legacy_stages.tag_definitions == 1 and legacy_stages.codec_definitions == 1 and legacy_stages.codec_tables == 1 and legacy_stages.semantic_definitions == 0 and legacy_stages.tag_variants.len() == 46 and legacy_stages.codec_variants == legacy_stages.tag_variants {
    } else if has_legacy_stage_layout { invalid(source_path, 1, "unrecognized handwritten pipeline stage layout") } else { invalid(source_path, 1, "unrecognized generated pipeline stage schema layout") }
  }
  let allowances = dead_code_allowance_analysis(source_path, lexed, lookup, attrs, count)?
  var reported_blanket_lines: Map[Bool] = {}
  let unique = collect {
    for found in allowances.blanket { if ! (reported_blanket_lines.get(f"{found.line}") ?? false) { yield found; reported_blanket_lines[f"{found.line}"] = true } }
  }
  {metrics: [metric("instruction_variants", instructions), metric("pattern_variants", patterns), metric("stage_variants", stages), metric("manual_payload_calls", calls), metric("optional_raw_calls", optional), metric("wide_arena_wildcards", wildcards), metric("static_type_predicate_definitions", predicates), metric("type_storage_mapping_definitions", mappings), metric("lowering_type_resolver_definitions", resolvers), metric("nested_machine_factory_calls", factories), metric("blanket_dead_code_lines", unique)], item_allowances: allowances.items}
}

## Scans one physical source file without executing Rust or inspecting literal text as code.
export proc scan_file(source_path: Str, source: Str) [error] -> Result[SourceScan, Error] {
  let lexed = lex(source_path, source)?
  let attrs = attributes(source_path, lexed)?
  let spans = excluded_spans(source_path, lexed, attrs)?
  let lookup = span_lookup(spans)
  let count = count_lines(source_path, lexed, spans, lookup)
  let analysis = structural(source_path, lexed, lookup, attrs, count)?
  let blanket = analysis.metrics |> where .name == "blanket_dead_code_lines" |> collect
  let allowed = blanket[0].evidence |> map .line |> collect
  let edges = collect {
    for edge in modules(source_path, lexed, attrs, lookup)? {
      yield {name: edge.name, directory: edge.directory, explicit_path: edge.explicit_path, excluded: edge.excluded, dead_code: edge.line in allowed, line: edge.line}
    }
  }
  {count: count, modules: edges, metrics: analysis.metrics, item_allowances: analysis.item_allowances}
}

proc module_target(root: Path, owner: Str, edge: RustModule) [fs, error] -> Result[Str] {
    var candidates = []
  if edge.explicit_path != null { candidates = [if edge.explicit_path.starts_with("/") { fp"{edge.explicit_path}" } else { fp"{root}/{edge.directory}/{edge.explicit_path}" }] } else { candidates = [fp"{root}/{edge.directory}/{edge.name}.rs", fp"{root}/{edge.directory}/{edge.name}/mod.rs"] }
  let existing = collect { for candidate in candidates { yield candidate.resolve()?.strip_prefix(root)?.display() when candidate.exists() } }
  if existing.len() != 1 { return Err(MetricError.Invalid(path: owner, line: edge.line, detail: f"module {edge.name} resolves to {existing.len()} files")) }
  existing[0]
}

## Measures both production source roots; external test modules are removed through their module edges.
export proc measure(root: Path, source_roots: List[Str]) [fs, error] -> Result[Report, Error] {
  measure_with_allowances(root, source_roots)?.report
}

## The separate allowance inventory uses the same live-module and inheritance traversal as the report.
export proc measure_with_allowances(root: Path, source_roots: List[Str]) [fs, error] -> Result[Measurement, Error] {
  guard (source_roots |> sort) == ["crates/xsht/src", "src"] else { return Err(MetricError.Contract(detail: "source roots must be exactly src and crates/xsht/src")) }
  let base = root.resolve()?
  var scans: List[SourceScan] = []
  var sizes: Map[Int] = {}
  var referenced: Map[Bool] = {}
  var active: Map[Bool] = {}
  var inherited: Map[Bool] = {}
  var edges: List[ModuleEdge] = []
  for source_root in source_roots {
    for entry in fs.walk(fp"{base}/{source_root}")? |> where .kind == "file" and .path.ext() == "rs" |> sort-by .path {
      let source_path = entry.path.strip_prefix(base)?.display()
      let source = entry.path.read_text()?
      let scan = scan_file(source_path, source)?
      sizes[source_path] = source.count_chars()
      scans += [scan]
      for edge in scan.modules {
        let target = module_target(base, source_path, edge)?
        referenced[target] = true
        edges += [{owner: source_path, target: target, excluded: edge.excluded, dead_code: edge.dead_code}]
      }
    }
  }
  for scan in scans { if ! (referenced.get(scan.count.path) ?? false) { active[scan.count.path] = true } }
  var changed = true
  while changed {
    changed = false
    for edge in edges {
      if ! edge.excluded and (active.get(edge.owner) ?? false) and ! (active.get(edge.target) ?? false) { active[edge.target] = true; changed = true }
      if ! edge.excluded and (active.get(edge.owner) ?? false) and (edge.dead_code or (inherited.get(edge.owner) ?? false)) and ! (inherited.get(edge.target) ?? false) { inherited[edge.target] = true; changed = true }
    }
  }
  let pattern_schema_rows = collect {
    for scan in scans {
      if active.get(scan.count.path) ?? false {
        for row in scan.metrics {
          if row.name == "pattern_variants" {
            for found in row.evidence { yield found when found.detail.starts_with("pattern_schema::") }
          }
        }
      }
    }
  }
  let legacy_pattern_rows = collect {
    for scan in scans {
      if active.get(scan.count.path) ?? false {
        for row in scan.metrics {
          if row.name == "pattern_variants" {
            for found in row.evidence { yield found when found.detail.starts_with("BuildPatternRow::") }
          }
        }
      }
    }
  }
  let active_pattern_owners = collect {
    for scan in scans {
        if (active.get(scan.count.path) ?? false) and scan.count.path in ["src/runtime/eval.rs", "src/runtime/eval/indexed/full/schema.rs"] { yield scan.count.path }
    }
  }
  let has_pattern_owner = !active_pattern_owners.is_empty()
  if ! pattern_schema_rows.is_empty() and ! legacy_pattern_rows.is_empty() {
    return Err(MetricError.Invalid(path: pattern_schema_rows[0].path, line: pattern_schema_rows[0].line, detail: "pattern schema overlaps the handwritten BuildPatternRow enum"))
  }
  if pattern_schema_rows.len() > PATTERN_SCHEMA_VARIANTS.len() {
    return Err(MetricError.Invalid(path: pattern_schema_rows[0].path, line: pattern_schema_rows[0].line, detail: "multiple generated pattern schema tables are active"))
  }
  if has_pattern_owner and pattern_schema_rows.is_empty() and legacy_pattern_rows.is_empty() {
    return Err(MetricError.Invalid(path: "src/runtime/eval/indexed/full/schema.rs", line: 1, detail: "unrecognized pattern schema authority layout"))
  }
  var sources: List[SourceCount] = []
  var item_allowances: List[ItemAllowance] = []
  var totals: Map[Int] = {}
  var occurrences: Map[List[Evidence]] = {}
  for scan in scans {
    let live = active.get(scan.count.path) ?? false
    let count = if live { scan.count } else { {path: scan.count.path, physical: scan.count.physical, production: 0, retained_lines: [], excluded: [{start: 0, end: sizes[scan.count.path], line: 1, last_line: scan.count.physical}]} }
    sources += [count]
    let source_group = if count.path.starts_with("src/") { "source_lines_src" } else { "source_lines_xsht" }
    totals[source_group] = (totals.get(source_group) ?? 0) + count.production
    occurrences[source_group] = (occurrences.get(source_group) ?? []) + [{path: count.path, line: 1, detail: f"{count.production} retained physical lines"}]
    for row in scan.metrics {
      let found = if row.name == "blanket_dead_code_lines" and (inherited.get(count.path) ?? false) {
        collect { for line in count.retained_lines { yield {path: count.path, line: line, detail: "inherited allow(dead_code) module scope"} } }
      } else { row.evidence }
      let amount = if row.name == "blanket_dead_code_lines" { found.len() } else { row.value }
      totals[row.name] = (totals.get(row.name) ?? 0) + (if live { amount } else { 0 })
      occurrences[row.name] = (occurrences.get(row.name) ?? []) + (if live { found } else { [] })
    }
    if live {
      for item in scan.item_allowances { item_allowances += [{...item, inherited_module_scope: inherited.get(count.path) ?? false}] }
    }
  }
  let metrics = collect { for name in totals.keys() |> sort { yield {name: name, value: totals[name], evidence: occurrences[name]} } }
  {report: {schema: 1, sources: sources, metrics: metrics, reviews: REVIEW_REQUIREMENTS}, item_allowances: {schema: 1, entries: item_allowances}}
}

## Fails on a structural rise or a changed metric schema; baseline files decode through Report.
export proc assert_not_risen(current: Report, baseline: Report) [error] -> Result[Unit, Error] {
  guard current.schema == baseline.schema else { return Err(MetricError.Contract(detail: "metric schema changed")) }
  let names = current.metrics |> map .name |> sort
  let previous = baseline.metrics |> map .name |> sort
  if names != previous { return Err(MetricError.Contract(detail: "metric inventory changed")) }
  for row in current.metrics {
    let old = baseline.metrics |> where .name == row.name |> collect
    if old.len() != 1 { return Err(MetricError.Contract(detail: f"duplicate metric {row.name}")) }
    if row.value < 0 or old[0].value < 0 { return Err(MetricError.Contract(detail: f"negative metric {row.name}")) }
    if row.value > old[0].value { return Err(MetricError.Rise(name: row.name, before: old[0].value, after: row.value)) }
  }
}

pure metric_value(report: Report, name: Str) -> Result[Int, Error] {
  let rows = report.metrics |> where .name == name |> collect
  if rows.len() != 1 { return Err(MetricError.Contract(detail: f"expected one {name} metric")) }
  rows[0].value
}

# Source totals remain in reports and audits but do not participate in structural rise checks.
proc validate_policy_report(report: Report) [error] {
  guard report.schema == 1 else { return Err(MetricError.Contract(detail: "unsupported consolidation metric schema")) }
  assert_not_risen(report, report)
  for name in SOURCE_ADVISORY_METRICS {
    let rows = report.metrics |> where .name == name |> collect
    if rows.len() != 1 { return Err(MetricError.Contract(detail: f"expected exactly one {name} metric")) }
  }
}

pure structural_report(report: Report) -> Report {
  {...report, metrics: collect { for row in report.metrics { yield row when row.name not in SOURCE_ADVISORY_METRICS } }}
}

## Intermediate and final acceptance use one structural ratchet while keeping source totals advisory.
export proc assert_structural_not_risen(current: Report, baseline: Report) [error] -> Result[Unit, Error] {
  validate_policy_report(current)
  validate_policy_report(baseline)
  assert_not_risen(structural_report(current), structural_report(baseline))
}

## Final close requires zero manual readers, optional raw reads, wide arena matches, and blanket scopes.
export proc assert_final_close(current: Report, baseline: Report) [error] -> Result[Unit, Error] {
  for name in FINAL_ZERO_METRICS {
    let _ = metric_value(current, name)?
    let _ = metric_value(baseline, name)?
  }
  assert_structural_not_risen(current, baseline)
  for name in FINAL_ZERO_METRICS {
      guard metric_value(current, name)? == 0 else {
      return Err(MetricError.Contract(detail: f"final close requires zero {name}"))
    }
  }
}

## The active observer must retain the frozen baseline proof and attest its own byte-identical report.
export proc verify_observer_successor(root: Path, baseline_path: Str, provenance_path: Str, baseline: Report) [fs, error] -> Result[Unit, Error] {
  let provenance_file = fp"{root}/dev/consolidation/metric-observer-successor.json"
  let provenance = json.read(provenance_file)?.require(ObserverSuccessorProvenance)?
  if provenance.schema != 1 or provenance.original_scanner_path != "dev/consolidation_metrics.xsh" or provenance.successor_scanner_path != "dev/consolidation_metrics_current.xsh" or provenance.original_baseline_path != "dev/consolidation/baseline.json" or provenance.corrected_baseline_path != "dev/consolidation/baseline-blanket-scope.json" {
    return Err(MetricError.Contract(detail: "unsupported or noncanonical current observer provenance"))
  }
  if provenance.same_baseline.item_inventory_path != "dev/consolidation/item-allowances-start.json" {
    return Err(MetricError.Contract(detail: "current observer provenance names a noncanonical item inventory"))
  }
  if provenance.same_baseline.report_path != "dev/consolidation/blanket-metric-start-diagnostic.json" {
    return Err(MetricError.Contract(detail: "current observer provenance names a noncanonical start report"))
  }
  let original_scanner_hash = hash.sha256(fp"{root}/{provenance.original_scanner_path}")?.hex().lower()
  let successor_scanner_hash = hash.sha256(fp"{root}/{provenance.successor_scanner_path}")?.hex().lower()
  let original_baseline_hash = hash.sha256(fp"{root}/{provenance.original_baseline_path}")?.hex().lower()
  let corrected_baseline_hash = hash.sha256(fp"{root}/{provenance.corrected_baseline_path}")?.hex().lower()
  let supplied_baseline_hash = hash.sha256(fp"{baseline_path}")?.hex().lower()
  let metadata_path = fp"{root}/dev/consolidation/baseline-metadata.json"
  let metadata_hash = hash.sha256(metadata_path)?.hex().lower()
  let historical_path = fp"{root}/dev/consolidation/blanket-metric-provenance.json"
  let historical_hash = hash.sha256(historical_path)?.hex().lower()
  let baseline_provenance_hash = hash.sha256(fp"{provenance_path}")?.hex().lower()
  if provenance.current_ratchet_tests_path != "dev/tests/test-consolidation-ratchets-current.xsh" {
    return Err(MetricError.Contract(detail: "current observer provenance names a noncanonical current ratchet fixture"))
  }
  let current_tests_hash = hash.sha256(fp"{root}/{provenance.current_ratchet_tests_path}")?.hex().lower()
  if original_scanner_hash != provenance.original_scanner_sha256.lower() or successor_scanner_hash != provenance.successor_scanner_sha256.lower() {
    return Err(MetricError.Contract(detail: "current observer provenance scanner hash mismatch"))
  }
  if original_baseline_hash != provenance.original_baseline_sha256.lower() or corrected_baseline_hash != provenance.corrected_baseline_sha256.lower() or supplied_baseline_hash != provenance.corrected_baseline_sha256.lower() {
    return Err(MetricError.Contract(detail: "current observer provenance baseline hash mismatch"))
  }
  if metadata_hash != provenance.metadata_sha256.lower() or historical_hash != provenance.historical_provenance_sha256.lower() or baseline_provenance_hash != provenance.baseline_provenance_sha256.lower() {
    return Err(MetricError.Contract(detail: "current observer provenance historical proof hash mismatch"))
  }
  let historical = json.read(historical_path)?.require(BlanketBaselineProvenance)?
  let passed = json.read(fp"{provenance_path}")?.require(BlanketBaselineProvenance)?
  let metadata = json.read(metadata_path)?.require(BlanketBaselineMetadata)?
  if historical.scanner_sha256.lower() != provenance.original_scanner_sha256.lower() or passed.scanner_sha256.lower() != provenance.original_scanner_sha256.lower() or historical.ratchet_tests_sha256.lower() != provenance.historical_ratchet_tests_sha256.lower() {
    return Err(MetricError.Contract(detail: "current observer provenance does not preserve the historical scanner attestation"))
  }
  if passed.ratchet_tests_sha256.lower() != provenance.baseline_ratchet_tests_sha256.lower() {
    return Err(MetricError.Contract(detail: "current observer provenance does not preserve the source advisory ratchet attestation"))
  }
  if current_tests_hash != provenance.current_ratchet_tests_sha256.lower() {
    return Err(MetricError.Contract(detail: "current observer provenance ratchet fixture hash mismatch"))
  }
  if metadata.start_commit != provenance.start_commit or metadata.source_manifest_sha256.lower() != provenance.source_manifest_sha256.lower() {
    return Err(MetricError.Contract(detail: "current observer provenance source identity mismatch"))
  }
  let corrected = json.read(fp"{root}/{provenance.corrected_baseline_path}")?.require(Report)?
  if baseline != corrected { return Err(MetricError.Contract(detail: "current observer was given a different corrected baseline")) }
  let report_path = fp"{root}/{provenance.same_baseline.report_path}"
  let report_hash = hash.sha256(report_path)?.hex().lower()
  let measured_baseline = json.read(report_path)?.require(Report)?
  if report_hash != provenance.same_baseline.report_sha256.lower() or provenance.same_baseline.legacy_report_sha256.lower() != report_hash or provenance.same_baseline.successor_report_sha256.lower() != report_hash {
    return Err(MetricError.Contract(detail: "old and current observers did not produce the corrected baseline report"))
  }
  let metric_values = collect { for row in measured_baseline.metrics { yield {name: row.name, value: row.value} } }
  if provenance.same_baseline.metric_values != metric_values {
    return Err(MetricError.Contract(detail: "current observer baseline metric proof mismatch"))
  }
  if provenance.same_baseline.source_count != measured_baseline.sources.len() or provenance.same_baseline.source_count != baseline.sources.len() {
    return Err(MetricError.Contract(detail: "current observer baseline source inventory proof mismatch"))
  }
  let item_inventory_path = fp"{root}/{provenance.same_baseline.item_inventory_path}"
  let item_inventory_hash = hash.sha256(item_inventory_path)?.hex().lower()
  let item_inventory = json.read(item_inventory_path)?.require(AllowanceInventory)?
  if item_inventory_hash != provenance.same_baseline.item_inventory_sha256.lower() or item_inventory.entries.len() != provenance.same_baseline.item_inventory_entries {
    return Err(MetricError.Contract(detail: "current observer baseline item inventory proof mismatch"))
  }
}

## A companion ratchet baseline is accepted only when its original source identity and scanner inputs remain pinned.
export proc verify_blanket_baseline(root: Path, baseline_path: Str, provenance_path: Str, baseline: Report) [fs, error] -> Result[Unit, Error] {
  let provenance = json.read(fp"{provenance_path}")?.require(BlanketBaselineProvenance)?
  if provenance.schema != 1 or provenance.original_baseline_path != "dev/consolidation/baseline.json" or provenance.corrected_baseline_path != "dev/consolidation/baseline-blanket-scope.json" {
    return Err(MetricError.Contract(detail: "unsupported or noncanonical corrected blanket baseline provenance"))
  }
  let original_path = fp"{root}/{provenance.original_baseline_path}"
  let corrected_path = fp"{root}/{provenance.corrected_baseline_path}"
  let metadata_path = fp"{root}/dev/consolidation/baseline-metadata.json"
  let original_hash = hash.sha256(original_path)?.hex().lower()
  let corrected_hash = hash.sha256(fp"{baseline_path}")?.hex().lower()
  let companion_hash = hash.sha256(corrected_path)?.hex().lower()
  let scanner_hash = hash.sha256(fp"{root}/dev/consolidation_metrics.xsh")?.hex().lower()
  if original_hash != provenance.original_baseline_sha256.lower() or corrected_hash != provenance.corrected_baseline_sha256.lower() or companion_hash != provenance.corrected_baseline_sha256.lower() {
    return Err(MetricError.Contract(detail: "corrected blanket baseline or preserved original baseline hash mismatch"))
  }
  if scanner_hash != provenance.scanner_sha256.lower() {
    return Err(MetricError.Contract(detail: "corrected blanket baseline scanner hash mismatch"))
  }
  let metadata = json.read(metadata_path)?.require(BlanketBaselineMetadata)?
  if metadata.start_commit != provenance.start_commit or metadata.source_manifest_sha256.lower() != provenance.source_manifest_sha256.lower() or metadata.baseline_report_sha256.lower() != provenance.original_baseline_sha256.lower() {
    return Err(MetricError.Contract(detail: "corrected blanket baseline no longer matches the original start-source identity"))
  }
  let original = json.read(original_path)?.require(Report)?
  if original.schema != baseline.schema or original.sources != baseline.sources or original.reviews != baseline.reviews or original.metrics.len() != baseline.metrics.len() {
    return Err(MetricError.Contract(detail: "corrected blanket baseline changes unrelated report structure"))
  }
  var blanket_rows = 0
  for old in original.metrics {
    let updated = baseline.metrics |> where .name == old.name |> collect
    if updated.len() != 1 { return Err(MetricError.Contract(detail: f"corrected blanket baseline changes metric inventory at {old.name}")) }
    if old.name == "blanket_dead_code_lines" {
      blanket_rows += 1
      if updated[0].value != provenance.corrected_blanket_lines { return Err(MetricError.Contract(detail: "corrected blanket baseline count does not match its provenance")) }
    } else if updated[0] != old {
      return Err(MetricError.Contract(detail: f"corrected blanket baseline changes unrelated metric {old.name}"))
    }
  }
  if blanket_rows != 1 { return Err(MetricError.Contract(detail: "corrected blanket baseline must contain exactly one blanket metric")) }
  verify_observer_successor(root, baseline_path, provenance_path, baseline)
}

pure valid_object_id(value: Str) -> Bool {
  return false when value.byte_len() not in [40, 64]
  for digit in value { guard digit.lower() in "0123456789abcdef" else { return false } }
  true
}

proc git_output(root: Path, arguments: List[Str]) [process, error] -> Result[Str, Error] {
  let argv = ["git", "-C", root.display(), @arguments]
  let output = run.capture --text --timeout=30s @argv
  let code = output.status.shell_code()?
  if code != 0 { return Err(MetricError.Contract(detail: f"git {arguments.join(" ")} failed with {code}: {output.stderr.trim()}")) }
  output.stdout.trim()
}

proc commit_source_oids(root: Path, commit: Str) [process, error] -> Result[List[ReviewSource], Error] {
  collect {
    for source_path in REVIEW_SOURCE_PATHS {
      let oid = git_output(root, ["rev-parse", "--verify", f"{commit}:{source_path}"])?
      yield {path: source_path, oid: oid}
    }
  }
}

proc verify_artifact(root: Path, artifact: ReviewArtifact) [fs, error] {
  if artifact.path.trim() != artifact.path or artifact.path == "" or artifact.path.starts_with("/") or "\\" in artifact.path {
    return Err(MetricError.Contract(detail: "review artifact path must be workspace-relative"))
  }
  for segment in artifact.path.split("/") {
    if segment in ["", ".", ".."] { return Err(MetricError.Contract(detail: "review artifact path escapes or aliases the workspace")) }
  }
  if artifact.sha256.byte_len() != 64 { return Err(MetricError.Contract(detail: "review artifact SHA-256 must be 64 hexadecimal characters")) }
  for digit in artifact.sha256 { guard digit.lower() in "0123456789abcdef" else { return Err(MetricError.Contract(detail: "review artifact SHA-256 is not hexadecimal")) } }
  if artifact.command.trim() == "" or artifact.method.trim() == "" or artifact.result.trim() != "verified" {
    return Err(MetricError.Contract(detail: "review artifact needs a command, method, and verified reviewer result"))
  }
  let base = root.resolve()?
  let resolved = fp"{base}/{artifact.path}".resolve()?
  let _ = resolved.strip_prefix(base)?
  if hash.sha256(resolved)?.hex().lower() != artifact.sha256.lower() {
    return Err(MetricError.Contract(detail: f"review artifact hash mismatch: {artifact.path}"))
  }
}

## Evidence stays separate from the fixed requirements; code and test trees bind it to reviewed behavior.
export proc verify_review_evidence(current: Report, bundle: ReviewEvidenceBundle, root: Path) [fs, process, error] -> Result[Unit, Error] {
  guard bundle.schema == 1 else { return Err(MetricError.Contract(detail: "unsupported review evidence schema")) }
  let expected_names = REVIEW_REQUIREMENTS |> map .name |> sort
  let current_names = current.reviews |> map .name |> sort
  if current_names != expected_names { return Err(MetricError.Contract(detail: "current report does not contain the canonical review rows")) }
  let proof_names = bundle.rows |> map .name |> sort
  if bundle.rows.len() != expected_names.len() or proof_names != expected_names {
    return Err(MetricError.Contract(detail: "review evidence must contain each canonical row exactly once"))
  }
  if ! valid_object_id(bundle.source_commit) { return Err(MetricError.Contract(detail: "review source_commit must be a full hexadecimal Git object ID")) }
  let base = root.resolve()?
  let head = git_output(base, ["rev-parse", "--verify", "HEAD"])?
  let source_commit = git_output(base, ["rev-parse", "--verify", bundle.source_commit])?
  if source_commit.lower() != bundle.source_commit.lower() or git_output(base, ["cat-file", "-t", source_commit])? != "commit" {
    return Err(MetricError.Contract(detail: "review source_commit is not an existing Git commit"))
  }
  let ancestry = ["git", "-C", base.display(), "merge-base", "--is-ancestor", source_commit, head]
  let ancestry_output = run.capture --text --timeout=30s @ancestry
  if ancestry_output.status.shell_code()? != 0 {
    return Err(MetricError.Contract(detail: "review source_commit is not in the current history"))
  }
  let dirty_paths = git_output(base, ["status", "--porcelain", "--untracked-files=all", "--", @REVIEW_SOURCE_PATHS])?
  if dirty_paths != "" { return Err(MetricError.Contract(detail: "reviewed source or test trees have uncommitted changes")) }
  let required_source_paths = REVIEW_SOURCE_PATHS |> sort
  let recorded_source_paths = bundle.sources |> map .path |> sort
  if recorded_source_paths != required_source_paths or bundle.sources.len() != required_source_paths.len() {
    return Err(MetricError.Contract(detail: "review evidence has missing or duplicate source identities"))
  }
  let source_oids = commit_source_oids(base, source_commit)?
  let current_oids = commit_source_oids(base, head)?
  for source_path in REVIEW_SOURCE_PATHS {
    let recorded = bundle.sources |> where .path == source_path |> collect
    let from_commit = source_oids |> where .path == source_path |> collect
    let current_source = current_oids |> where .path == source_path |> collect
    if recorded.len() != 1 or from_commit.len() != 1 or current_source.len() != 1 or ! valid_object_id(recorded[0].oid) or recorded[0].oid.lower() != from_commit[0].oid.lower() or recorded[0].oid.lower() != current_source[0].oid.lower() {
      return Err(MetricError.Contract(detail: f"review evidence is stale for source {source_path}"))
    }
  }
  for row in bundle.rows {
    if row.status != "review_verified" or row.reviewer.trim() == "" or row.evidence.is_empty() {
      return Err(MetricError.Contract(detail: f"review row is not verified with evidence: {row.name}"))
    }
    for artifact in row.evidence { verify_artifact(base, artifact) }
  }
}
