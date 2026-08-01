#!/usr/bin/env -S xsh --
# Perf Collapse
# Collapse perf script callchains into folded stacks for flamegraph rendering.
# Usage: xsh showcase/perf-collapse.xsh -- PERF_SCRIPT [--top N]
# Example: xsh showcase/perf-collapse.xsh -- perf.script > out.folded
error ScriptError = Failed(kind: Str, message: Str)

# Collapse `perf script` callchains into folded stacks for flamegraph rendering.
#
# Usage:
#   perf record -F 999 -g -- target/debug/xsh examples/streams.xsh
#   perf script > out.perf
#   xsh showcase/perf-collapse.xsh -- out.perf > out.folded
#   xsh showcase/flamegraph.xsh -- out.folded > out.svg
#
# With no argument, collapses a built-in sample.
type Options = {input: Path, comm: Str, include: Str, exclude: Str, top: Int, leaf_first: Bool}

pure usage() -> Str {
  return "usage: xsh showcase/perf-collapse.xsh -- PERF_SCRIPT [--comm NAME] [--include REGEX] [--exclude REGEX] [--top N] [--leaf-first]"
}

pure clean_symbol(raw: Str) -> Str {
  let trimmed = raw.trim()
  let without_offset = trimmed.split("+0x")[0]
  let without_semis = without_offset.replace(";", ":")
  let without_open = without_semis.replace("(", "")
  let symbol = without_open.replace(")", "")

  if symbol == "" {
    return "[unknown]"
  }

  return symbol
}

pure frame_symbol(line: Str) -> Str {
  let fields = line.trim().fields()

  if fields.len() == 0 {
    return ""
  }

  if fields.len() == 1 {
    return clean_symbol(fields[0])
  }

  let first = fields[0]
  let second = fields[1]
  let looks_like_addr = first.starts_with("0x") or fields.len() >= 3

  if looks_like_addr {
    return clean_symbol(second)
  }

  return clean_symbol(first)
}

pure stack_key(stack: List[Str], leaf_first: Bool) -> Result[Str] {
  if leaf_first {
    return stack.join(";")
  }

  var out: List[Str] = []
  var i = stack.len() - 1

  while i >= 0 {
    out = out.push(stack[i])
    i -= 1
  }

  return out.join(";")
}

pure comm_from_header(line: Str) -> Str {
  let words = line.trim().fields()

  if words.len() == 0 {
    return ""
  }

  return words[0]
}

pure parse_options(argv: List[Str]) -> Result[Options] {
  let parsed = cli.applet(
    argv,
    {
      comm: {
        form: "--comm NAME",
        default: "",
      },
      include: {
        form: "--include REGEX",
        default: "",
      },
      exclude: {
        form: "--exclude REGEX",
        default: "",
      },
      top: {
        form: "--top N",
        kind: "Int",
        default: 0,
      },
      leaf_first: {
        form: "--leaf-first",
        default: false,
      },
      operands: {
        form: "...FILE",
      },
    },
  )?
  let input = parsed.operands.get(0, "")

  if parsed.operands.len() != 1 {
    return Err(ScriptError.Failed("usage", usage()))
  }

  return {
    input: fp"${input}",
    comm: parsed.comm,
    include: parsed.include,
    exclude: parsed.exclude,
    top: parsed.top,
    leaf_first: parsed.leaf_first,
  }
}

pure collapse_text(source: Str, opts: Options) -> Result[Map[Int]] {
  let include_re = regex.compile(if opts.include == "" { ".*" } else { opts.include })?
  let exclude_re = regex.compile(if opts.exclude == "" { "a^" } else { opts.exclude })?
  var counts: Map[Int] = {}
  var current_comm = ""
  var current_stack: List[Str] = []

  for line in source.lines() {
    if line.trim() == "" {
      if current_stack.len() > 0 and (opts.comm == "" or current_comm == opts.comm) {
        let key = stack_key(current_stack, opts.leaf_first)?

        if include_re.matches(key) and ! exclude_re.matches(key) {
          counts[key] = counts.get(key, 0) + 1
        }
      }

      current_stack = []
      current_comm = ""
      continue
    }

    if line.starts_with(" ") or line.starts_with("\t") {
      let symbol = frame_symbol(line)

      if symbol != "" {
        current_stack = current_stack.push(symbol)
      }

      continue
    }

    if current_stack.len() > 0 and (opts.comm == "" or current_comm == opts.comm) {
      let key = stack_key(current_stack, opts.leaf_first)?

      if include_re.matches(key) and ! exclude_re.matches(key) {
        counts[key] = counts.get(key, 0) + 1
      }
    }

    current_stack = []
    current_comm = comm_from_header(line)
  }

  if current_stack.len() > 0 and (opts.comm == "" or current_comm == opts.comm) {
    let key = stack_key(current_stack, opts.leaf_first)?

    if include_re.matches(key) and ! exclude_re.matches(key) {
      counts[key] = counts.get(key, 0) + 1
    }
  }

  return counts
}

proc print_folded(counts: Map[Int], top: Int) [error] {
  let rows = [{stack: key, count: counts.get(key, 0)} for key in counts.keys()]
  let sorted = rows |> sort-by --desc .count
  let limit = if top <= 0 or top > sorted.len() { sorted.len() } else { top }

  for row in sorted |> take(limit) {
    print f"${row.stack} ${row.count}"
  }
}

proc main(...argv: List[Str]) [fs, error] {
  let sample = """xsh 1242 [000] 10.000000: cycles:
        0000000000000000 alloc::raw_vec::RawVec<T,A>::grow_one (/work/target/debug/xsh)
        0000000000000000 xsh::runtime::value::Value::clone (/work/target/debug/xsh)
        0000000000000000 xsh::runtime::eval::stream::eval_map (/work/target/debug/xsh)
        0000000000000000 xsh::runtime::eval::Eval::eval_program (/work/target/debug/xsh)

xsh 1242 [000] 10.001000: cycles:
        0000000000000000 alloc::raw_vec::RawVec<T,A>::grow_one (/work/target/debug/xsh)
        0000000000000000 xsh::runtime::value::Value::clone (/work/target/debug/xsh)
        0000000000000000 xsh::runtime::eval::stream::eval_map (/work/target/debug/xsh)
        0000000000000000 xsh::runtime::eval::Eval::eval_program (/work/target/debug/xsh)

xsh 1242 [000] 10.002000: cycles:
        0000000000000000 alloc::collections::btree::map::BTreeMap<K,V,A>::insert (/work/target/debug/xsh)
        0000000000000000 xsh::modules::fs::fs_entry_record (/work/target/debug/xsh)
        0000000000000000 xsh::runtime::eval::Eval::eval_program (/work/target/debug/xsh)
"""

  if argv.len() == 0 {
    let opts: Options = {
      input: p"",
      comm: "",
      include: "",
      exclude: "",
      top: 0,
      leaf_first: false,
    }

    print_folded(collapse_text(sample, opts)?, 0)
    return
  }

  if argv[0] == "--help" or argv[0] == "-h" {
    print usage()
    return
  }

  let opts = parse_options(argv)?
  let counts = collapse_text(opts.input.read_text()?, opts)?
  print_folded(counts, opts.top)
}
