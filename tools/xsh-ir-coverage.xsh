type CoverageRow = {name: Str, covered: Int, total: Int, percent: Int, supported: List[Str], unsupported: List[Str]}

type Options = {root: Path, json: Str}

type LoweredNodeCounts = {statements: Int, expressions: Int, pipeline_stages: Int, types: Int}

type PureScan = {path: Str, line: Int, name: Str, lowerable: Bool, reasons: List[Str]}

type ProcScan = {path: Str, line: Int, name: Str, effects: List[Str], lowerable: Bool, reasons: List[Str]}

type ScriptScan = {path: Str, line: Int, shape: Str, lowerable: Bool, reasons: List[Str]}

type ReasonCount = {reason: Str, count: Int}

type ReasonGroup = {group: Str, total: Int, reasons: List[ReasonCount]}

type CorpusReport = {
  roots: List[Str],
  total: Int,
  lowerable: Int,
  percent: Int,
  reasons: List[ReasonCount],
  groups: List[ReasonGroup],
  samples: List[PureScan],
}

type ProcReport = {
  roots: List[Str],
  total: Int,
  lowerable: Int,
  percent: Int,
  reasons: List[ReasonCount],
  groups: List[ReasonGroup],
  samples: List[ProcScan],
}

type ScriptReport = {
  roots: List[Str],
  total: Int,
  lowerable: Int,
  percent: Int,
  reasons: List[ReasonCount],
  groups: List[ReasonGroup],
  samples: List[ScriptScan],
}

type CoverageReport = {
  rows: List[CoverageRow],
  lowered_nodes: LoweredNodeCounts,
  lowered_methods: List[Str],
  corpus: CorpusReport,
  procs: ProcReport,
  script: ScriptReport,
}

type CodeDelimiterScan = {brace_delta: Int, delimiter_delta: Int, in_triple_string: Bool}

pure count_char(text: Str, needle: Str) -> Int {
  return text.count_chars() - text.replace(needle, "").count_chars()
}

pure brace_delta(line: Str) -> Int {
  return count_char(line, "{") - count_char(line, "}")
}

pure delimiter_delta(line: Str) -> Int {
  return brace_delta(line) + count_char(line, "[") - count_char(line, "]") + count_char(line, "(") - count_char(
    line,
    ")",
  )
}

pure variant_name(line: Str) -> Str {
  let trimmed = line.trim()

  if trimmed == "" or trimmed.starts_with("}") or trimmed.starts_with("//") {
    return ""
  }

  return trimmed.replace("{", " ").replace("(", " ").replace(",", " ").fields().get(0, "")
}

pure enum_variants(source: Str, enum_name: Str) -> List[Str] {
  let public_marker = f"pub enum ${enum_name}"
  let private_marker = f"enum ${enum_name}"
  var variants: List[Str] = []
  var in_enum = false
  var depth = 0

  for raw in source.lines() {
    let line = raw.trim()

    if ! in_enum {
      if line.starts_with(public_marker) or line.starts_with(private_marker) {
        in_enum = true
        depth = brace_delta(line)
      }

      continue
    }

    if depth == 1 {
      let name = variant_name(line)

      if name != "" {
        variants = variants.push(name)
      }
    }

    depth += brace_delta(line)

    if depth <= 0 {
      return variants
    }
  }

  return variants
}

pure list_intersection(left: List[Str], right: List[Str]) -> List[Str] {
  [item for item in left if right.contains(item)]
}

pure list_difference(left: List[Str], right: List[Str]) -> List[Str] {
  [item for item in left if ! right.contains(item)]
}

pure percent(covered: Int, total: Int) -> Int {
  if total == 0 {
    return 0
  }

  return covered * 100 / total
}

pure coverage_row(name: Str, all: List[Str], supported: List[Str]) -> CoverageRow {
  let covered = list_intersection(all, supported) |> sort
  let unsupported = list_difference(all, supported) |> sort

  return {
    name: name,
    covered: covered.len(),
    total: all.len(),
    percent: percent(covered.len(), all.len()),
    supported: covered,
    unsupported: unsupported,
  }
}

pure quoted_tokens(line: Str) -> List[Str] {
  let parts = line.split("\"")
  var values: List[Str] = []
  var index = 1

  while index < parts.len() {
    let token = parts.get(index, "")

    if token != "" {
      values = values.push(token)
    }

    index += 2
  }

  return values
}

pure lowered_method_names(source: Str) -> List[Str] {
  var methods: List[Str] = []
  var in_list = false

  for raw in source.lines() {
    let line = raw.trim()

    if ! in_list {
      if line.starts_with("const LOWERED_METHOD_NAMES") {
        in_list = true
      }

      continue
    }

    for token in quoted_tokens(line) {
      if ! methods.contains(token) {
        methods = methods.push(token)
      }
    }

    # The list is a `const LOWERED_METHOD_NAMES: &[&str] = &[ ... ];` array, so
    # the closing bracket terminates it.
    if line.contains("]") {
      return methods |> sort
    }
  }

  return methods |> sort
}

pure triple_quote_count(line: Str) -> Int {
  return line.split("\"\"\"").len() - 1
}

pure code_delimiter_scan(line: Str, start_in_triple_string: Bool) -> CodeDelimiterScan {
  var brace = 0
  var delimiter = 0
  var in_triple_string = start_in_triple_string
  let parts = line.split("\"\"\"")
  var index = 0

  for part in parts {
    if ! in_triple_string {
      brace += brace_delta(part)
      delimiter += delimiter_delta(part)
    }

    if index + 1 < parts.len() {
      in_triple_string = ! in_triple_string
    }

    index += 1
  }

  return {brace_delta: brace, delimiter_delta: delimiter, in_triple_string: in_triple_string}
}

pure standard_record_names(source: Str) -> List[Str] {
  var names: List[Str] = []

  for raw in source.lines() {
    let line = raw.trim()

    if line.starts_with("(\"") and line.contains("\",") {
      let name = line.split("\"").get(1, "")

      if name != "" and ! names.contains(name) {
        names = names.push(name)
      }
    }
  }

  return names |> sort
}

pure starts_pure(line: Str) -> Bool {
  let trimmed = line.trim()
  return trimmed.starts_with("pure ") or trimmed.starts_with("export pure ")
}

pure starts_proc(line: Str) -> Bool {
  let trimmed = line.trim()
  return trimmed.starts_with("proc ") or trimmed.starts_with("export proc ")
}

pure pure_name(signature: Str) -> Str {
  let trimmed = signature.trim().replace("export pure ", "pure ")
  let after_pure = trimmed.split("pure ").get(1, trimmed)
  return after_pure.split("(").get(0, after_pure).trim()
}

pure proc_name(signature: Str) -> Str {
  let trimmed = signature.trim().replace("export proc ", "proc ")
  let after_proc = trimmed.split("proc ").get(1, trimmed)
  return after_proc.split("(").get(0, after_proc).trim()
}

pure pure_function_names(source: Str) -> List[Str] {
  var names: List[Str] = []

  for raw in source.lines() {
    let line = raw.trim()

    if starts_pure(line) {
      let name = pure_name(line)

      if name != "" and ! names.contains(name) {
        names = names.push(name)
      }
    }
  }

  return names
}

pure proc_function_names(source: Str) -> List[Str] {
  var names: List[Str] = []

  for raw in source.lines() {
    let line = raw.trim()

    if starts_proc(line) {
      let name = proc_name(line)

      if name != "" and ! names.contains(name) {
        names = names.push(name)
      }
    }
  }

  return names
}

pure record_schema_names(source: Str) -> List[Str] {
  var names: List[Str] = []

  for raw in source.lines() {
    let line = raw.trim().replace("export type ", "type ")

    if line.starts_with("type ") and line.contains("= {") {
      let name = line.split("type ").get(1, "").split("=").get(0, "").trim()

      if name != "" and ! names.contains(name) {
        names = names.push(name)
      }
    }
  }

  return names
}

pure tag_union_names(source: Str) -> List[Str] {
  var names: List[Str] = []

  for raw in source.lines() {
    let line = raw.trim().replace("export type ", "type ")

    if line.starts_with("type ") and line.contains("=") and ! line.contains("= {") and line.contains("|") {
      let name = line.split("type ").get(1, "").split("=").get(0, "").trim()

      if name != "" and ! names.contains(name) {
        names = names.push(name)
      }
    }
  }

  return names
}

pure qualified_names(namespace: Str, names: List[Str]) -> List[Str] {
  var values: List[Str] = []

  for name in names {
    values = values.push(name)

    if namespace != "" {
      values = values.push(f"${namespace}.${name}")
    }
  }

  return values
}

pure module_namespace(path_text: Str) -> Str {
  if ! path_text.ends_with(".xsh") {
    return ""
  }

  let without_ext = path_text.split(".xsh").get(0, path_text)
  let parts = without_ext.split("/")

  if parts.len() < 2 {
    return ""
  }

  return parts.get(parts.len() - 1, "")
}

pure extend_unique(values: List[Str], extra: List[Str]) -> List[Str] {
  var result = values

  for item in extra {
    if item != "" and ! result.contains(item) {
      result = result.push(item)
    }
  }

  return result
}

pure error_variant_names(source: Str) -> List[Str] {
  var names: List[Str] = []

  for raw in source.lines() {
    let line = raw.trim().replace("export error ", "error ")

    if line.starts_with("error ") and line.contains("=") {
      let variants = line.split("=").get(1, "").split("|")

      for raw_variant in variants {
        let name = raw_variant.trim().replace("(", " ").fields().get(0, "")

        if name != "" and ! names.contains(name) {
          names = names.push(name)
        }
      }
    }
  }

  return names
}

pure lowerable_named_type(raw: Str, record_types: List[Str]) -> Bool {
  if raw == "Unit" or raw == "Int" or raw == "Bool" or raw == "Str" or raw == "Regex" or raw == "Status" or raw == "Path" or raw == "Error" or raw == "Record" {
    return true
  }

  if record_types.contains(raw) {
    return true
  }

  let short = raw.split(".").get(raw.split(".").len() - 1, raw)
  return record_types.contains(short)
}

pure lowerable_type(raw: Str, allow_result: Bool, record_types: List[Str]) -> Bool {
  let ty = raw.trim().replace("?", "")

  if lowerable_named_type(ty, record_types) {
    return true
  }

  if ty.starts_with("List[") or ty.starts_with("Map[") {
    return true
  }

  if allow_result and ty.starts_with("Result[") {
    let inner = ty.split("Result[").get(1, "").split("]").get(0, "")
    return lowerable_type(inner, false, record_types)
  }

  return false
}

pure add_reason(reasons: List[Str], reason: Str) -> List[Str] {
  if reason == "" or reasons.contains(reason) {
    return reasons
  }

  return reasons.push(reason)
}

pure signature_reasons(signature: Str, record_types: List[Str]) -> List[Str] {
  let normalized = signature.replace("(", " ").replace(")", " ").replace(",", " ").replace("{", " ")
  let tokens = normalized.fields()
  var reasons: List[Str] = []
  var index = 0

  while index < tokens.len() {
    let token = tokens.get(index, "")
    let next = tokens.get(index + 1, "")

    if token.ends_with(":") and next != "" and ! lowerable_type(next, false, record_types) {
      reasons = add_reason(reasons, f"type.param.${next}")
    }

    if token == "->" and next != "" and ! lowerable_type(next, true, record_types) {
      reasons = add_reason(reasons, f"type.return.${next}")
    }

    index += 1
  }

  return reasons
}

pure method_reasons(
  line: Str,
  lowered_methods: List[Str],
  error_variants: List[Str],
  pure_functions: List[Str],
) -> List[Str] {
  let parts = line.split(".")
  var reasons: List[Str] = []
  var index = 1

  while index < parts.len() {
    let part = parts.get(index, "")

    if part.contains("(") {
      let method = part.split("(").get(0, "").trim()
      let receiver = receiver_name(parts.get(index - 1, ""))
      let qualified = f"${receiver}.${method}"

      if ! known_module_receiver(receiver) and plausible_field_call_name(method) and ! lowered_methods.contains(method) and ! error_variants.contains(
        method,
      ) and ! pure_functions.contains(qualified) {
        reasons = add_reason(reasons, f"method.${method}")
      }
    }

    index += 1
  }

  return reasons
}

pure receiver_name(raw: Str) -> Str {
  let fields = raw.replace("(", " ").replace("{", " ").replace("[", " ").fields()

  if fields.len() == 0 {
    return ""
  }

  return fields.get(fields.len() - 1, "")
}

pure known_module_receiver(name: Str) -> Bool {
  let modules = [
    "Path",
    "archive",
    "bytes",
    "cli",
    "cpu",
    "dns",
    "env",
    "fs",
    "hash",
    "io",
    "json",
    "linux",
    "map",
    "mime",
    "process",
    "record",
    "regex",
    "set",
    "test",
    "time",
    "tui",
    "unix",
  ]

  return modules.contains(name)
}

pure plausible_field_call_name(name: Str) -> Bool {
  if name == "" {
    return false
  }

  if name.contains(" ") or name.contains("}") or name.contains("{") or name.contains("|") or name.contains(")") {
    return false
  }

  if name.contains("$") or name.contains("\"") or name.contains("'") or name.contains(":") {
    return false
  }

  if name.contains(",") or name.contains("/") or name.contains("[") or name.contains("]") or name.contains("+") {
    return false
  }

  if name.lower() != name {
    return false
  }

  return true
}

pure supported_lowered_pipeline_line(line: Str) -> Bool {
  let supported_stage = line.contains("|> take(") or line.contains("|> drop(") or line.contains("|> where ") or line.contains(
    "|> map ",
  ) or line.contains("|> enumerate()") or line.contains("|> sort") or line.contains("|> sort-by ") or line.contains(
    "|> group-by ",
  ) or line.contains("|> text.lines")

  if ! supported_stage {
    return false
  }

  if line.contains("{ |") and ! line.contains("|> map { |") {
    return false
  }

  let unsupported = [
    "|> par-map",
    "|> each",
    "|> batch",
    "|> first",
    "|> last",
    "|> unique-by",
    "|> zip",
    "|> range",
    "|> repeat",
    "|> tee",
    "|> sum",
    "|> min",
    "|> max",
    "|> fold",
    "|> reduce",
    "|> flat-map",
    "|> any",
    "|> all",
    "|> shuffle",
    "|> table.print",
    "|> bytes.chunks",
    "|> json.lines",
    "|> json.stream",
    "|> count",
    "|> reduce-by",
  ]

  for marker in unsupported {
    if line.contains(marker) {
      return false
    }
  }

  return true
}

pure body_line_reasons(
  line: Str,
  lowered_methods: List[Str],
  error_variants: List[Str],
  pure_functions: List[Str],
) -> List[Str] {
  let trimmed = line.trim()
  var reasons: List[Str] = []

  if trimmed == "" or trimmed.starts_with("#") {
    return reasons
  }

  if trimmed.starts_with("loop ") {
    reasons = add_reason(reasons, "stmt.loop")
  }

  if trimmed.starts_with("guard ") {
    reasons = add_reason(reasons, "stmt.guard")
  }

  if trimmed.starts_with("break ") {
    reasons = add_reason(reasons, "stmt.break")
  }

  if trimmed.starts_with("continue") {
    return reasons
  }

  if trimmed.contains("|>") and ! supported_lowered_pipeline_line(trimmed) {
    reasons = add_reason(reasons, "expr.pipeline")
  }

  for reason in method_reasons(trimmed, lowered_methods, error_variants, pure_functions) {
    reasons = add_reason(reasons, reason)
  }

  return reasons
}

pure pure_scan(
  script_path: Str,
  line: Int,
  signature: Str,
  body: Str,
  lowered_methods: List[Str],
  record_types: List[Str],
  error_variants: List[Str],
  pure_functions: List[Str],
) -> PureScan {
  var reasons = signature_reasons(signature, record_types)

  for body_line in body.lines() {
    for reason in body_line_reasons(body_line, lowered_methods, error_variants, pure_functions) {
      reasons = add_reason(reasons, reason)
    }
  }

  return {
    path: script_path,
    line: line,
    name: pure_name(signature),
    lowerable: reasons.len() == 0,
    reasons: reasons |> sort,
  }
}

pure proc_effects(signature: Str) -> List[Str] {
  let before_return = signature.split("->").get(0, signature).trim()

  if ! before_return.ends_with("]") {
    return ["unrestricted"]
  }

  let parts = before_return.split("[")
  let raw = parts.get(parts.len() - 1, "").split("]").get(0, "").trim()
  var effects: List[Str] = []

  if raw == "" {
    return effects
  }

  for effect in raw.split(",") {
    let name = effect.trim()

    if name != "" {
      effects = effects.push(name)
    }
  }

  return effects
}

pure lowerable_proc_effect_set(effects: List[Str]) -> Bool {
  for effect in effects {
    if effect != "error" {
      return false
    }
  }

  return true
}

pure proc_scan(
  script_path: Str,
  line: Int,
  signature: Str,
  body: Str,
  lowered_methods: List[Str],
  record_types: List[Str],
  error_variants: List[Str],
  pure_functions: List[Str],
) -> ProcScan {
  let effects = proc_effects(signature)
  var reasons = signature_reasons(signature, record_types)

  if ! lowerable_proc_effect_set(effects) {
    reasons = add_reason(reasons, f"effect.${effects.join("+")}")
  }

  for body_line in body.lines() {
    for reason in body_line_reasons(body_line, lowered_methods, error_variants, pure_functions) {
      reasons = add_reason(reasons, reason)
    }
  }

  return {
    path: script_path,
    line: line,
    name: proc_name(signature),
    effects: effects,
    lowerable: reasons.len() == 0,
    reasons: reasons |> sort,
  }
}

proc scan_pures_in_file(
  display_root: Path,
  script_path: Path,
  lowered_methods: List[Str],
  corpus_record_types: List[Str],
  corpus_error_variants: List[Str],
  corpus_pure_functions: List[Str],
) [fs, error] -> Result[List[PureScan]] {
  let text = fs.read_text(script_path)?
  let path_text = script_path.strip_prefix(display_root)?.display()
  let namespace = module_namespace(path_text)

  let record_types = extend_unique(
    extend_unique(corpus_record_types, qualified_names(namespace, record_schema_names(text))),
    qualified_names(namespace, tag_union_names(text)),
  )

  let error_variants = extend_unique(corpus_error_variants, error_variant_names(text))
  let pure_functions = extend_unique(corpus_pure_functions, qualified_names(namespace, pure_function_names(text)))
  var scans: List[PureScan] = []
  var in_pure = false
  var seen_body = false
  var depth = 0
  var line_no = 0
  var start_line = 0
  var signature = ""
  var body = ""
  var in_triple_string = false
  let newline = "\n"

  for raw in text.lines() {
    line_no += 1
    let line = raw.trim()
    let was_in_triple_string = in_triple_string
    let line_scan = code_delimiter_scan(line, in_triple_string)
    in_triple_string = line_scan.in_triple_string

    if ! in_pure {
      continue when was_in_triple_string or line.contains("\"\"\"")

      if starts_pure(line) {
        in_pure = true
        seen_body = line.contains("{")
        depth = line_scan.brace_delta
        start_line = line_no
        signature = line
        body = if seen_body { line } else { "" }

        if seen_body and depth <= 0 {
          scans = scans.push(
            pure_scan(
              path_text,
              start_line,
              signature,
              body,
              lowered_methods,
              record_types,
              error_variants,
              pure_functions,
            ),
          )

          in_pure = false
        }
      }

      continue
    }

    if ! seen_body {
      if line.starts_with("}") {
        in_pure = false
        continue
      }

      signature = f"${signature} ${line}"
      seen_body = line.contains("{")

      if seen_body {
        body = line
      }
    } else {
      body = f"${body}${newline}${line}"
    }

    depth += line_scan.brace_delta

    if seen_body and depth <= 0 {
      scans = scans.push(
        pure_scan(path_text, start_line, signature, body, lowered_methods, record_types, error_variants, pure_functions),
      )

      in_pure = false
    }
  }

  return scans
}

proc scan_procs_in_file(
  display_root: Path,
  script_path: Path,
  lowered_methods: List[Str],
  corpus_record_types: List[Str],
  corpus_error_variants: List[Str],
  corpus_lowerable_functions: List[Str],
) [fs, error] -> Result[List[ProcScan]] {
  let text = fs.read_text(script_path)?
  let path_text = script_path.strip_prefix(display_root)?.display()
  let namespace = module_namespace(path_text)

  let record_types = extend_unique(
    extend_unique(corpus_record_types, qualified_names(namespace, record_schema_names(text))),
    qualified_names(namespace, tag_union_names(text)),
  )

  let error_variants = extend_unique(corpus_error_variants, error_variant_names(text))

  let lowerable_functions = extend_unique(
    corpus_lowerable_functions,
    qualified_names(namespace, pure_function_names(text)),
  )

  var scans: List[ProcScan] = []
  var in_proc = false
  var seen_body = false
  var depth = 0
  var line_no = 0
  var start_line = 0
  var signature = ""
  var body = ""
  var in_triple_string = false
  let newline = "\n"

  for raw in text.lines() {
    line_no += 1
    let line = raw.trim()
    let was_in_triple_string = in_triple_string
    let line_scan = code_delimiter_scan(line, in_triple_string)
    in_triple_string = line_scan.in_triple_string

    if ! in_proc {
      continue when was_in_triple_string or line.contains("\"\"\"")

      if starts_proc(line) {
        in_proc = true
        seen_body = line.contains("{")
        depth = line_scan.brace_delta
        start_line = line_no
        signature = line
        body = if seen_body { line } else { "" }

        if seen_body and depth <= 0 {
          scans = scans.push(
            proc_scan(
              path_text,
              start_line,
              signature,
              body,
              lowered_methods,
              record_types,
              error_variants,
              lowerable_functions,
            ),
          )

          in_proc = false
        }
      }

      continue
    }

    if ! seen_body {
      if line.starts_with("}") {
        in_proc = false
        continue
      }

      signature = f"${signature} ${line}"
      seen_body = line.contains("{")

      if seen_body {
        body = line
      }
    } else {
      body = f"${body}${newline}${line}"
    }

    depth += line_scan.brace_delta

    if seen_body and depth <= 0 {
      scans = scans.push(
        proc_scan(
          path_text,
          start_line,
          signature,
          body,
          lowered_methods,
          record_types,
          error_variants,
          lowerable_functions,
        ),
      )

      in_proc = false
    }
  }

  return scans
}

pure add_reason_count(counts: Map[Int], reason: Str) -> Map[Int] {
  return counts.set(reason, counts.get(reason, 0) + 1)
}

pure reason_rows(counts: Map[Int]) -> List[ReasonCount] {
  [{reason: reason, count: counts.get(reason, 0)} for reason in counts.keys() |> sort]
}

pure reason_group(reason: Str) -> Str {
  if reason == "stmt.Use" {
    return "import-boundary"
  }

  if reason == "stmt.Command" or reason == "expr.run" or reason == "stmt.Defer" or reason == "stmt.SignalHook" {
    return "runtime-boundary"
  }

  if reason.starts_with("effect.") {
    return "effect-boundary"
  }

  if reason.starts_with("method.") {
    return "method"
  }

  if reason.starts_with("type.") {
    return "type"
  }

  if reason.starts_with("expr.") {
    return "expression"
  }

  if reason.starts_with("stmt.") {
    return "statement"
  }

  return "other"
}

pure reason_groups(rows: List[ReasonCount]) -> List[ReasonGroup] {
  let order = [
    "import-boundary",
    "runtime-boundary",
    "effect-boundary",
    "method",
    "type",
    "expression",
    "statement",
    "other",
  ]

  var groups: List[ReasonGroup] = []

  for bucket in order {
    var total = 0
    var reasons: List[ReasonCount] = []

    for row in rows {
      if reason_group(row.reason) == bucket {
        total += row.count
        reasons = reasons.push(row)
      }
    }

    if total > 0 {
      groups = groups.push({group: bucket, total: total, reasons: reasons})
    }
  }

  return groups
}

pure corpus_report(roots: List[Str], scans: List[PureScan]) -> CorpusReport {
  var lowerable = 0
  var counts: Map[Int] = {}
  var samples: List[PureScan] = []

  for scan in scans {
    if scan.lowerable {
      lowerable += 1
    } else {
      if samples.len() < 20 {
        samples = samples.push(scan)
      }

      for reason in scan.reasons {
        counts = add_reason_count(counts, reason)
      }
    }
  }

  let reasons = reason_rows(counts)

  return {
    roots: roots,
    total: scans.len(),
    lowerable: lowerable,
    percent: percent(lowerable, scans.len()),
    reasons: reasons,
    groups: reason_groups(reasons),
    samples: samples,
  }
}

pure proc_report(roots: List[Str], scans: List[ProcScan]) -> ProcReport {
  var lowerable = 0
  var counts: Map[Int] = {}
  var samples: List[ProcScan] = []

  for scan in scans {
    if scan.lowerable {
      lowerable += 1
    } else {
      if samples.len() < 20 {
        samples = samples.push(scan)
      }

      for reason in scan.reasons {
        counts = add_reason_count(counts, reason)
      }
    }
  }

  let reasons = reason_rows(counts)

  return {
    roots: roots,
    total: scans.len(),
    lowerable: lowerable,
    percent: percent(lowerable, scans.len()),
    reasons: reasons,
    groups: reason_groups(reasons),
    samples: samples,
  }
}

pure script_shape(line: Str) -> Str {
  let trimmed = line.trim()

  if trimmed == "" or trimmed.starts_with("#") or trimmed.starts_with("}") {
    return ""
  }

  let normalized = trimmed.replace("export ", "")

  if normalized.starts_with("use ") {
    return "Use"
  }

  if normalized.starts_with("type ") or normalized.starts_with("error ") or normalized.starts_with("proc ") or normalized.starts_with(
    "pure ",
  ) {
    return ""
  }

  if normalized.starts_with("signal ") or normalized.starts_with("on ") {
    return "SignalHook"
  }

  if normalized.starts_with("defer ") {
    return "Defer"
  }

  if normalized.starts_with("let ") {
    return "Let"
  }

  if normalized.starts_with("var ") {
    return "Var"
  }

  if normalized.starts_with("if ") {
    return "If"
  }

  if normalized.starts_with("while ") {
    return "While"
  }

  if normalized.starts_with("for ") {
    return "For"
  }

  if normalized.starts_with("match ") {
    return "Match"
  }

  if normalized.starts_with("guard ") {
    return "Guard"
  }

  if normalized.starts_with("with ") {
    return "With"
  }

  if normalized.starts_with("loop ") {
    return "Loop"
  }

  if normalized.starts_with("return") {
    return "Return"
  }

  if normalized.starts_with("break") {
    return "Break"
  }

  if normalized.starts_with("continue") {
    return "Continue"
  }

  if normalized.starts_with("print ") or normalized.starts_with("eprint ") or normalized.starts_with("run ") {
    return "Command"
  }

  if normalized.contains(" = ") or normalized.contains(" += ") or normalized.contains(" -= ") or normalized.contains(
    " *= ",
  ) or normalized.contains(" /= ") or normalized.contains(" %= ") {
    return "Assign"
  }

  if normalized.fields().len() == 1 and ! normalized.contains("(") {
    return "TailBareIdent"
  }

  return "Expr"
}

pure script_continuation_line(line: Str) -> Bool {
  let trimmed = line.trim()
  return trimmed.starts_with("|>")
}

pure non_executable_signature_start(line: Str) -> Bool {
  let normalized = line.trim().replace("export ", "")
  return normalized.starts_with("proc ") or normalized.starts_with("pure ")
}

pure append_scan_line(text: Str, line: Str) -> Str {
  if text == "" {
    return line
  }

  let newline = "\n"
  return f"${text}${newline}${line}"
}

pure script_supported_shape(shape: Str) -> Bool {
  return shape == "Let" or shape == "Var" or shape == "Assign" or shape == "If" or shape == "While" or shape == "For" or shape == "Match" or shape == "Expr"
}

pure script_region_reasons(
  shape: Str,
  text: Str,
  lowered_methods: List[Str],
  error_variants: List[Str],
  pure_functions: List[Str],
) -> List[Str] {
  var reasons: List[Str] = []

  if shape == "" {
    return reasons
  }

  if ! script_supported_shape(shape) {
    reasons = add_reason(reasons, f"stmt.${shape}")
  }

  if text.contains("run ") or text.contains(" run.") {
    reasons = add_reason(reasons, "expr.run")
  }

  for line in text.lines() {
    for reason in body_line_reasons(line, lowered_methods, error_variants, pure_functions) {
      reasons = add_reason(reasons, reason)
    }
  }

  return reasons
}

pure script_line_reasons(
  line: Str,
  lowered_methods: List[Str],
  error_variants: List[Str],
  pure_functions: List[Str],
) -> List[Str] {
  return script_region_reasons(script_shape(line), line, lowered_methods, error_variants, pure_functions)
}

pure script_report(roots: List[Str], scans: List[ScriptScan]) -> ScriptReport {
  var lowerable = 0
  var counts: Map[Int] = {}
  var samples: List[ScriptScan] = []

  for scan in scans {
    if scan.lowerable {
      lowerable += 1
    } else {
      if samples.len() < 20 {
        samples = samples.push(scan)
      }

      for reason in scan.reasons {
        counts = add_reason_count(counts, reason)
      }
    }
  }

  let reasons = reason_rows(counts)

  return {
    roots: roots,
    total: scans.len(),
    lowerable: lowerable,
    percent: percent(lowerable, scans.len()),
    reasons: reasons,
    groups: reason_groups(reasons),
    samples: samples,
  }
}

pure default_corpus_roots(root: Path) -> List[Path] {
  let parent = root.parent()
  return [root, fp"${parent}/packages", fp"${parent}/laputa"]
}

proc scan_corpus(root: Path, lowered_methods: List[Str]) [fs, error] -> Result[CorpusReport] {
  let display_root = root.parent()
  let records_path = fp"${root}/src/sema/records.rs"
  let standard_records = standard_record_names(fs.read_text(records_path)?)
  var roots: List[Str] = []
  var files: List[Path] = []
  var corpus_record_types = standard_records
  var corpus_error_variants: List[Str] = []
  var corpus_pure_functions: List[Str] = []
  var scans: List[PureScan] = []

  for corpus_root in default_corpus_roots(root) {
    continue unless corpus_root.exists()?
    roots = roots.push(corpus_root.strip_prefix(display_root)?.display())

    for entry in fs.walk(corpus_root)?
      |> where .kind == "file" and .path.ext() == "xsh"
      |> sort-by .path {
      files = files.push(entry.path)
      let text = fs.read_text(entry.path)?
      let path_text = entry.path.strip_prefix(display_root)?.display()
      let namespace = module_namespace(path_text)

      corpus_record_types = extend_unique(
        extend_unique(corpus_record_types, qualified_names(namespace, record_schema_names(text))),
        qualified_names(namespace, tag_union_names(text)),
      )

      corpus_error_variants = extend_unique(corpus_error_variants, error_variant_names(text))

      corpus_pure_functions = extend_unique(
        corpus_pure_functions,
        qualified_names(namespace, pure_function_names(text)),
      )
    }
  }

  for file in files {
    scans = scans.extend(
      scan_pures_in_file(
        display_root,
        file,
        lowered_methods,
        corpus_record_types,
        corpus_error_variants,
        corpus_pure_functions,
      )?,
    )
  }

  return corpus_report(roots, scans)
}

proc scan_proc_corpus(root: Path, lowered_methods: List[Str]) [fs, error] -> Result[ProcReport] {
  let display_root = root.parent()
  let records_path = fp"${root}/src/sema/records.rs"
  let standard_records = standard_record_names(fs.read_text(records_path)?)
  var roots: List[Str] = []
  var files: List[Path] = []
  var corpus_record_types = standard_records
  var corpus_error_variants: List[Str] = []
  var corpus_lowerable_functions: List[Str] = []
  var scans: List[ProcScan] = []

  for corpus_root in default_corpus_roots(root) {
    continue unless corpus_root.exists()?
    roots = roots.push(corpus_root.strip_prefix(display_root)?.display())

    for entry in fs.walk(corpus_root)?
      |> where .kind == "file" and .path.ext() == "xsh"
      |> sort-by .path {
      files = files.push(entry.path)
      let text = fs.read_text(entry.path)?
      let path_text = entry.path.strip_prefix(display_root)?.display()
      let namespace = module_namespace(path_text)

      corpus_record_types = extend_unique(
        extend_unique(corpus_record_types, qualified_names(namespace, record_schema_names(text))),
        qualified_names(namespace, tag_union_names(text)),
      )

      corpus_error_variants = extend_unique(corpus_error_variants, error_variant_names(text))

      corpus_lowerable_functions = extend_unique(
        corpus_lowerable_functions,
        qualified_names(namespace, pure_function_names(text)),
      )

      corpus_lowerable_functions = extend_unique(
        corpus_lowerable_functions,
        qualified_names(namespace, proc_function_names(text)),
      )
    }
  }

  for file in files {
    scans = scans.extend(
      scan_procs_in_file(
        display_root,
        file,
        lowered_methods,
        corpus_record_types,
        corpus_error_variants,
        corpus_lowerable_functions,
      )?,
    )
  }

  return proc_report(roots, scans)
}

proc scan_script_statements_in_file(
  display_root: Path,
  script_path: Path,
  lowered_methods: List[Str],
  corpus_error_variants: List[Str],
  corpus_pure_functions: List[Str],
) [fs, error] -> Result[List[ScriptScan]] {
  let text = fs.read_text(script_path)?
  let path_text = script_path.strip_prefix(display_root)?.display()
  var scans: List[ScriptScan] = []
  var pending_text = ""
  var pending_shape = ""
  var pending_line = 0
  var pending_depth = 0
  var skip_depth = 0
  var skip_signature = false
  var in_triple_string = false
  var line_no = 0

  for raw in text.lines() {
    line_no += 1
    let line = raw.trim()
    let was_in_triple_string = in_triple_string
    let line_scan = code_delimiter_scan(line, in_triple_string)
    in_triple_string = line_scan.in_triple_string

    if skip_depth > 0 {
      skip_depth += line_scan.brace_delta

      if skip_depth < 0 {
        skip_depth = 0
      }

      continue
    }

    if skip_signature {
      if line.contains("{") {
        skip_depth = line_scan.brace_delta

        if skip_depth < 0 {
          skip_depth = 0
        }

        skip_signature = false
      }

      continue
    }

    if pending_text != "" {
      if pending_depth > 0 {
        pending_text = append_scan_line(pending_text, line)
        pending_depth += line_scan.delimiter_delta

        if pending_depth < 0 {
          pending_depth = 0
        }

        continue
      }

      if script_continuation_line(line) {
        pending_text = append_scan_line(pending_text, line)
        pending_depth += line_scan.delimiter_delta

        if pending_depth < 0 {
          pending_depth = 0
        }

        continue
      }

      continue when was_in_triple_string or in_triple_string

      let reasons = script_region_reasons(
        pending_shape,
        pending_text,
        lowered_methods,
        corpus_error_variants,
        corpus_pure_functions,
      )

      scans = scans.push(
        {path: path_text, line: pending_line, shape: pending_shape, lowerable: reasons.len() == 0, reasons: reasons},
      )

      pending_text = ""
      pending_shape = ""
      pending_line = 0
      pending_depth = 0
    }

    let shape = script_shape(line)

    if shape != "" {
      pending_text = line
      pending_shape = shape
      pending_line = line_no
      pending_depth = line_scan.delimiter_delta

      if pending_depth < 0 {
        pending_depth = 0
      }
    } else {
      if non_executable_signature_start(line) and ! line.contains("{") {
        skip_signature = true
        continue
      }

      skip_depth = line_scan.brace_delta

      if skip_depth < 0 {
        skip_depth = 0
      }
    }
  }

  if pending_text != "" {
    let reasons = script_region_reasons(
      pending_shape,
      pending_text,
      lowered_methods,
      corpus_error_variants,
      corpus_pure_functions,
    )

    scans = scans.push(
      {path: path_text, line: pending_line, shape: pending_shape, lowerable: reasons.len() == 0, reasons: reasons},
    )
  }

  return scans
}

proc scan_script_corpus(root: Path, lowered_methods: List[Str]) [fs, error] -> Result[ScriptReport] {
  let display_root = root.parent()
  var roots: List[Str] = []
  var files: List[Path] = []
  var corpus_error_variants: List[Str] = []
  var corpus_pure_functions: List[Str] = []
  var scans: List[ScriptScan] = []

  for corpus_root in default_corpus_roots(root) {
    continue unless corpus_root.exists()?
    roots = roots.push(corpus_root.strip_prefix(display_root)?.display())

    for entry in fs.walk(corpus_root)?
      |> where .kind == "file" and .path.ext() == "xsh"
      |> sort-by .path {
      files = files.push(entry.path)
      let text = fs.read_text(entry.path)?
      let path_text = entry.path.strip_prefix(display_root)?.display()
      let namespace = module_namespace(path_text)
      corpus_error_variants = extend_unique(corpus_error_variants, error_variant_names(text))

      corpus_pure_functions = extend_unique(
        corpus_pure_functions,
        qualified_names(namespace, pure_function_names(text)),
      )
    }
  }

  for file in files {
    scans = scans.extend(
      scan_script_statements_in_file(display_root, file, lowered_methods, corpus_error_variants, corpus_pure_functions)?,
    )
  }

  return script_report(roots, scans)
}

pure render_row(row: CoverageRow) -> List[Str] {
  var lines = [f"${row.name}: ${row.covered}/${row.total} (${row.percent}%)"]
  lines = lines.push(f"  supported: ${row.supported.join(", ")}")

  if row.unsupported.len() == 0 {
    lines = lines.push("  unsupported: none")
  } else {
    lines = lines.push(f"  unsupported: ${row.unsupported.join(", ")}")
  }

  return lines
}

pure render_reason_groups(groups: List[ReasonGroup]) -> List[Str] {
  var lines: List[Str] = []

  if groups.len() == 0 {
    return lines
  }

  lines = lines.push("  fallback groups:")

  for bucket in groups {
    lines = lines.push(f"    ${bucket.group}: ${bucket.total}")
  }

  return lines
}

pure render_report(report: CoverageReport) -> Str {
  var lines = ["lowered IR coverage", "scope: static AST surface compared to the current lowered IR capability map", ""]

  for row in report.rows {
    lines = lines.extend(render_row(row))
    lines = lines.push("")
  }

  lines = lines.push("lowered IR nodes")
  lines = lines.push(f"  statements: ${report.lowered_nodes.statements}")
  lines = lines.push(f"  expressions: ${report.lowered_nodes.expressions}")
  lines = lines.push(f"  pipeline stages: ${report.lowered_nodes.pipeline_stages}")
  lines = lines.push(f"  types: ${report.lowered_nodes.types}")
  lines = lines.push("")
  lines = lines.push(f"lowered method whitelist: ${report.lowered_methods.len()}")
  lines = lines.push(f"  ${report.lowered_methods.join(", ")}")
  lines = lines.push("")
  lines = lines.push("corpus pure-function lowerability")
  lines = lines.push(f"  roots: ${report.corpus.roots.join(", ")}")
  lines = lines.push(f"  lowerable: ${report.corpus.lowerable}/${report.corpus.total} (${report.corpus.percent}%)")

  if report.corpus.reasons.len() == 0 {
    lines = lines.push("  fallback reasons: none")
  } else {
    lines = lines.push("  fallback reasons:")

    for row in report.corpus.reasons {
      lines = lines.push(f"    ${row.reason}: ${row.count}")
    }
  }

  lines = lines.extend(render_reason_groups(report.corpus.groups))

  if report.corpus.samples.len() > 0 {
    lines = lines.push("  non-lowerable samples:")

    for scan in report.corpus.samples {
      lines = lines.push(f"    ${scan.path}:${scan.line} ${scan.name} -> ${scan.reasons.join(", ")}")
    }
  }

  lines = lines.push("")
  lines = lines.push("corpus effect-free proc-body lowerability")
  lines = lines.push(f"  roots: ${report.procs.roots.join(", ")}")
  lines = lines.push(f"  lowerable: ${report.procs.lowerable}/${report.procs.total} (${report.procs.percent}%)")

  if report.procs.reasons.len() == 0 {
    lines = lines.push("  fallback reasons: none")
  } else {
    lines = lines.push("  fallback reasons:")

    for row in report.procs.reasons {
      lines = lines.push(f"    ${row.reason}: ${row.count}")
    }
  }

  lines = lines.extend(render_reason_groups(report.procs.groups))

  if report.procs.samples.len() > 0 {
    lines = lines.push("  non-lowerable samples:")

    for scan in report.procs.samples {
      lines = lines.push(
        f"    ${scan.path}:${scan.line} ${scan.name} [${scan.effects.join(", ")}] -> ${scan.reasons.join(", ")}",
      )
    }
  }

  lines = lines.push("")
  lines = lines.push("corpus top-level script lowerability")
  lines = lines.push(f"  roots: ${report.script.roots.join(", ")}")
  lines = lines.push(f"  lowerable: ${report.script.lowerable}/${report.script.total} (${report.script.percent}%)")

  if report.script.reasons.len() == 0 {
    lines = lines.push("  fallback reasons: none")
  } else {
    lines = lines.push("  fallback reasons:")

    for row in report.script.reasons {
      lines = lines.push(f"    ${row.reason}: ${row.count}")
    }
  }

  lines = lines.extend(render_reason_groups(report.script.groups))

  if report.script.samples.len() > 0 {
    lines = lines.push("  non-lowerable samples:")

    for scan in report.script.samples {
      lines = lines.push(f"    ${scan.path}:${scan.line} ${scan.shape} -> ${scan.reasons.join(", ")}")
    }
  }

  lines = lines.push("")
  lines = lines.push("notes")

  lines = lines.push(
    "  This is not whole-language runtime coverage. Process forms, modules, streams, tracing-sensitive calls, unrestricted procs, and OS effects are intentionally outside the lowered fast path.",
  )

  lines = lines.push(
    "  Use the unsupported lists as the next IR expansion map; use benchmark deltas to decide whether a supported construct is worth optimizing further.",
  )

  lines = lines.push("")
  return lines.join("\n")
}

let opts: Options = cli.parse(
  args,
  {root: {form: "--root PATH", default: p"."}, json: {form: "--json PATH", default: ""}},
)?

let root = opts.root.resolve()?
let arena_path = fp"${root}/src/syntax/arena.rs"
let node_path = fp"${root}/src/syntax/node.rs"
let eval_path = fp"${root}/src/runtime/eval.rs"
let arena_source = fs.read_text(arena_path)?
let node_source = fs.read_text(node_path)?
let eval_source = fs.read_text(eval_path)?
let stmt_variants = enum_variants(arena_source, "ArenaStmtKind")
let expr_variants = enum_variants(arena_source, "ArenaExprKind")
let type_variants = enum_variants(arena_source, "ArenaTypeExprTag")
let binary_variants = enum_variants(node_source, "BinaryOp")
let assign_variants = enum_variants(node_source, "AssignOp")
let lowered_stmt_variants = enum_variants(eval_source, "LoweredStmt")
let lowered_expr_variants = enum_variants(eval_source, "LoweredExpr")
let lowered_pipeline_stage_variants = enum_variants(eval_source, "LoweredPipelineStage")
let lowered_type_variants = enum_variants(eval_source, "LoweredType")
let lowered_methods = lowered_method_names(eval_source)

let rows = [
  coverage_row(
    "statement variants",
    stmt_variants,
    [
      "Let",
      "Var",
      "Assign",
      "If",
      "While",
      "For",
      "Match",
      "Return",
      "Break",
      "Continue",
    ],
  ),
  coverage_row(
    "expression variants",
    expr_variants,
    [
      "Bool",
      "Int",
      "Str",
      "FmtString",
      "Ident",
      "Item",
      "List",
      "ListComp",
      "StructuredPipeline",
      "Record",
      "Binary",
      "Call",
      "Field",
      "Index",
      "If",
      "Match",
      "Try",
    ],
  ),
  coverage_row("type expression variants", type_variants, ["Named", "List", "Map", "Result"]),
  coverage_row(
    "binary operators",
    binary_variants,
    [
      "Eq",
      "Ne",
      "Lt",
      "Le",
      "Gt",
      "Ge",
      "And",
      "Or",
      "In",
      "NotIn",
      "ResultFallback",
      "Add",
      "Sub",
      "Mul",
      "Div",
      "Rem",
    ],
  ),
  coverage_row("assignment operators", assign_variants, ["Set", "Add", "Sub", "Mul", "Div", "Rem"]),
]

let report: CoverageReport = {
  rows: rows,
  lowered_nodes: {
    statements: lowered_stmt_variants.len(),
    expressions: lowered_expr_variants.len(),
    pipeline_stages: lowered_pipeline_stage_variants.len(),
    types: lowered_type_variants.len(),
  },
  lowered_methods: lowered_methods,
  corpus: scan_corpus(root, lowered_methods)?,
  procs: scan_proc_corpus(root, lowered_methods)?,
  script: scan_script_corpus(root, lowered_methods)?,
}

let text = render_report(report)
print $text

if opts.json != "" {
  let json_path = fp"${opts.json}"
  json_path.parent().mkdir()?
  json.write(json_path, report)?
}
