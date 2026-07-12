# Summarize repeated monomorphized functions in a cargo-llvm-lines table.
#
# Each generic function monomorphized more than once is a "repeat offender":
# collapsing its copies into a single non-generic inner function reclaims the
# duplicated lines. This script lists those offenders and, with --sum, reports
# the reclaimable total as a share of the module's llvm-lines (TOTAL).
#
# Typical use after an existing measurement:
#
#   cargo llvm-lines --release --no-default-features --features tools --lib > /tmp/xsh-llvm-lines.txt
#   target/release/xsh tools/llvm-lines-repeat-offenders.xsh -- /tmp/xsh-llvm-lines.txt --limit 40 --examples 1
#   target/release/xsh tools/llvm-lines-repeat-offenders.xsh -- /tmp/xsh-llvm-lines.txt --sum               # owned reclaimable total
#   target/release/xsh tools/llvm-lines-repeat-offenders.xsh -- /tmp/xsh-llvm-lines.txt --sum --all         # + dependencies
#   target/release/xsh tools/llvm-lines-repeat-offenders.xsh -- /tmp/xsh-llvm-lines.txt --sum --all --filter slice::sort
#
# Standalone measurement mode:
#
#   target/debug/xsh tools/llvm-lines-repeat-offenders.xsh -- --generate --all --limit 40
#
# By default, only functions owned by the xsh crate are shown. Pass --all to include
# std/dependency items, which is often useful when looking for call sites that pull
# in generic library machinery such as sort or BTreeMap bulk construction.
#
# --filter SUBSTRING restricts both the table and the --sum total to offenders
# whose normalized name contains the substring (e.g. "slice::sort", "btree").
# --sum switches from the per-offender table to a one-line reclaimable-lines
# total (and a percentage of the llvm-lines (TOTAL)); combine with --json for a
# machine-readable summary object.
#
# The generated-mode artifact is intentionally bounded: the script analyzes the
# complete captured llvm-lines output in memory, then stores only the header and the
# top --artifact-rows individual llvm-lines rows under /tmp/xsh-llvm-lines-<pid>/.
error LlvmLinesError = Failed(message: Str)

type Options = {
  input: Str,
  limit: Int,
  min_instances: Int,
  min_total_lines: Int,
  min_duplicated: Int,
  examples: Int,
  all: Bool,
  json: Bool,
  sum: Bool,
  generate: Bool,
  artifact_rows: Int,
  artifact: Str,
  filter: Str,
}

type MonoRow = {name: Str, original: Str, lines: Int, copies: Int}

type InputText = {text: Str, artifact: Str}

type Offender = {
  name: Str,
  instances: Int,
  total_lines: Int,
  duplicated_lines: Int,
  max_instance_lines: Int,
  copies: Int,
  examples: List[Str],
}

pure is_digit_byte(ch: Int) -> Bool {
  return ch >= 48 and ch <= 57
}

pure is_decimal(text: Str) -> Bool {
  if text == "" {
    return false
  }

  var index = 0

  while index < text.byte_len() {
    if ! is_digit_byte(text.byte_at(index, -1)) {
      return false
    }

    index += 1
  }

  return true
}

pure normalize_function_name(name: Str) -> Str {
  let parts = name.split("::<")

  if parts.len() > 1 {
    return f"${parts[0]}::<_>"
  }

  return name
}

pure is_project_owned(name: Str) -> Bool {
  return name.starts_with("xsh[") or name.starts_with("<xsh[")
}

pure parse_mono_row(line: Str) -> Result[MonoRow] {
  let trimmed = line.trim()
  let fields = trimmed.fields()
  let original = (fields |> drop(6)).join(" ")
  let lines: Int = json.decode(fields[0])?
  let copies: Int = json.decode(fields[3])?
  return Ok({name: normalize_function_name(original), original: original, lines: lines, copies: copies})
}

pure is_llvm_lines_row(line: Str) -> Bool {
  let trimmed = line.trim()

  if trimmed == "" or ! is_digit_byte(trimmed.byte_at(0, -1)) {
    return false
  }

  let fields = trimmed.fields()
  return fields.len() >= 7 and is_decimal(fields[0]) and is_decimal(fields[3])
}

pure maybe_add_example(examples: List[Str], value: Str, limit: Int) -> List[Str] {
  if examples.len() < limit and ! (value in examples) {
    return examples.push(value)
  }

  return examples
}

pure option_takes_value(arg: Str) -> Bool {
  return arg == "--limit" or arg == "--min-instances" or arg == "--min-total-lines" or arg == "--examples" or arg == "--artifact-rows" or arg == "--artifact"
}

pure has_generate_arg(argv: List[Str]) -> Bool {
  return "--generate" in argv
}

pure has_positional_input(argv: List[Str]) -> Bool {
  var skip_next = false

  for arg in argv {
    if skip_next {
      skip_next = false
      continue
    }

    if option_takes_value(arg) {
      skip_next = true
      continue
    }

    if ! arg.starts_with("--") {
      return true
    }
  }

  return false
}

pure argv_for_parse(argv: List[Str]) -> List[Str] {
  if has_generate_arg(argv) and ! has_positional_input(argv) {
    return argv.push("-")
  }

  return argv
}

pure maybe_push_offender(
  offenders: List[Offender],
  name: Str,
  instances: Int,
  total_lines: Int,
  max_instance_lines: Int,
  copies: Int,
  examples: List[Str],
  min_instances: Int,
  min_total_lines: Int,
) -> List[Offender] {
  let duplicated_lines = total_lines - max_instance_lines

  if instances >= min_instances and total_lines >= min_total_lines and duplicated_lines > 0 {
    return offenders.push({
      name: name,
      instances: instances,
      total_lines: total_lines,
      duplicated_lines: duplicated_lines,
      max_instance_lines: max_instance_lines,
      copies: copies,
      examples: examples,
    })
  }

  return offenders
}

pure offenders_from_text(
  text: Str,
  min_instances: Int,
  min_total_lines: Int,
  example_limit: Int,
  include_dependencies: Bool,
) -> Result[List[Offender]] {
  var rows: List[MonoRow] = []

  for line in text.lines() {
    let trimmed = line.trim()

    if "::<" in trimmed and is_llvm_lines_row(trimmed) {
      let row = parse_mono_row(trimmed)?

      if include_dependencies or is_project_owned(row.name) {
        rows = rows.push(row)
      }
    }
  }

  let sorted = rows |> sort-by .name
  var offenders: List[Offender] = []
  var active = false
  var name = ""
  var instances = 0
  var total_lines = 0
  var max_instance_lines = 0
  var copies = 0
  var examples: List[Str] = []

  for row in sorted {
    if active and row.name != name {
      offenders = maybe_push_offender(
        offenders,
        name,
        instances,
        total_lines,
        max_instance_lines,
        copies,
        examples,
        min_instances,
        min_total_lines,
      )

      examples = []
      instances = 0
      total_lines = 0
      max_instance_lines = 0
      copies = 0
    }

    if ! active or row.name != name {
      active = true
      name = row.name
    }

    instances += 1
    total_lines += row.lines
    copies += row.copies
    max_instance_lines = if row.lines > max_instance_lines { row.lines } else { max_instance_lines }
    examples = maybe_add_example(examples, row.original, example_limit)
  }

  if active {
    offenders = maybe_push_offender(
      offenders,
      name,
      instances,
      total_lines,
      max_instance_lines,
      copies,
      examples,
      min_instances,
      min_total_lines,
    )
  }

  return offenders |> sort-by --desc .duplicated_lines
}

pure total_from_llvm_lines(text: Str) -> Result[Int] {
  for line in text.lines() {
    let trimmed = line.trim()

    if "(TOTAL)" in trimmed {
      let fields = trimmed.fields()

      if fields.len() >= 1 {
        let total: Int = json.decode(fields[0])?
        return Ok(total)
      }
    }
  }

  -1
}

pure filter_offenders(rows: List[Offender], filter: Str, min_duplicated: Int) -> List[Offender] {
  var out: List[Offender] = []

  for row in rows {
    if row.duplicated_lines >= min_duplicated {
      if filter == "" or filter in row.name {
        out = out.push(row)
      }
    }
  }

  return out
}

pure bounded_llvm_lines_artifact(text: Str, artifact_rows: Int) -> Str {
  let max_rows = if artifact_rows < 0 { 0 } else { artifact_rows }
  var output: List[Str] = []
  var rows = 0

  for line in text.lines() {
    if is_llvm_lines_row(line) {
      if rows < max_rows {
        output = output.push(line)
      }

      rows += 1
      continue
    }

    if rows == 0 {
      output = output.push(line)
    }
  }

  output = output.push(f"# truncated: kept top ${max_rows} individual llvm-lines rows from ${rows} rows")

  return f"""${output.join("\n")}
"""
}

proc generated_input(artifact: Str, artifact_rows: Int) [fs, process, error, io] -> Result[InputText] {
  let captured = run.capture --text cargo llvm-lines --release --no-default-features --features tools --lib ?

  if captured.stderr != "" {
    io.write_stdout(captured.stderr)?
  }

  if ! captured.status.ok {
    return Err(LlvmLinesError.Failed("cargo llvm-lines failed"))
  }

  var artifact_path = /tmp/xsh-llvm-lines.txt

  if artifact == "" {
    let pid = process.current_pid()?
    let dir = fp"/tmp/xsh-llvm-lines-${pid}"

    if ! dir.exists()? {
      dir.mkdir()?
    }

    artifact_path = fp"${dir}/xsh-llvm-lines.txt"
  } else {
    artifact_path = fp"${artifact}"
  }

  artifact_path.parent().mkdir()?
  artifact_path.write(bounded_llvm_lines_artifact(captured.stdout, artifact_rows))?
  return Ok({text: captured.stdout, artifact: artifact_path.display()})
}

proc read_input(input: Str) [fs, error, io] -> Result[InputText] {
  if input == "-" {
    return Ok({text: io.stdin_text()?, artifact: ""})
  }

  let input_path = fp"${input}"
  return Ok({text: fs.read_text(input_path)?, artifact: input_path.display()})
}

proc print_text(rows: List[Offender], limit: Int, artifact: Str) [io] {
  let shown = if limit <= 0 or limit > rows.len() { rows.len() } else { limit }

  if artifact != "" {
    print f"llvm-lines artifact: ${artifact}"
  }

  print f"top ${shown} llvm-lines repeat offenders"
  print f"  ${"duplicated":>10} ${"total":>10} ${"inst":>5} ${"max":>8}  function"

  for row in rows |> take(shown) {
    print f"  ${row.duplicated_lines:>10} ${row.total_lines:>10} ${row.instances:>5} ${row.max_instance_lines:>8}  ${row.name}"

    for example in row.examples {
      if example != row.name {
        print f"      ${example}"
      }
    }
  }
}

proc print_summary(
  rows: List[Offender],
  filter: Str,
  include_dependencies: Bool,
  grand: Int,
  use_json: Bool,
) [error, io] {
  var duplicated = 0
  var instances = 0

  for row in rows {
    duplicated += row.duplicated_lines
    instances += row.instances
  }

  let scope = if include_dependencies { "all" } else { "owned" }
  let dup = duplicated
  let g = grand
  let pct = if g > 0 { dup.float() * 100.0 / g.float() } else { 0.0 }

  if use_json {
    io.write_stdout(
      json.encode(
        {
          filter: filter,
          scope: scope,
          offenders: rows.len(),
          instances: instances,
          duplicated: duplicated,
          grand_total: grand,
          pct: pct,
        },
        pretty: true,
      )?,
    )?
  } else if g > 0 {
    print f"scope=${scope} filter=\"${filter}\" offenders=${rows.len()} instances=${instances} duplicated=${duplicated} (${pct}% of ${grand})"
  } else {
    print f"scope=${scope} filter=\"${filter}\" offenders=${rows.len()} instances=${instances} duplicated=${duplicated}"
  }
}

proc main(...argv: List[Str]) [fs, process, error, io] {
  let opts: Options = cli.parse(
    argv_for_parse(argv),
    {
      input: {
        form: "INPUT",
        default: "-",
      },
      limit: {
        form: "--limit N",
        default: 30,
      },
      min_instances: {
        form: "--min-instances N",
        default: 2,
      },
      min_total_lines: {
        form: "--min-total-lines N",
        default: 0,
      },
      min_duplicated: {
        form: "--min-duplicated N",
        default: 0,
      },
      examples: {
        form: "--examples N",
        default: 2,
      },
      artifact_rows: {
        form: "--artifact-rows N",
        default: 200,
      },
      artifact: {
        form: "--artifact PATH",
        default: "",
      },
      filter: {
        kind: "Str",
        default: "",
      },
      all: "Bool",
      json: "Bool",
      sum: "Bool",
      generate: "Bool",
    },
  )?

  let input = if opts.generate { generated_input(opts.artifact, opts.artifact_rows)? } else { read_input(opts.input)? }
  let rows = offenders_from_text(input.text, opts.min_instances, opts.min_total_lines, opts.examples, opts.all)?
  let filtered = filter_offenders(rows, opts.filter, opts.min_duplicated)

  if opts.sum {
    let grand = match total_from_llvm_lines(input.text) { Ok(n) => n, Err(_) => -1 }
    print_summary(filtered, opts.filter, opts.all, grand, opts.json)
  } else {
    let shown = if opts.limit <= 0 or opts.limit > filtered.len() { filtered } else { filtered |> take(opts.limit) }

    if opts.json {
      io.write_stdout(json.encode({artifact: input.artifact, rows: shown}, pretty: true)?)?
    } else {
      print_text(filtered, opts.limit, input.artifact)
    }
  }
}
