error ScriptError = Failed(kind: Str, message: Str)

type AllocationMetrics = {
  allocation_calls: Int,
  allocation_bytes: Int,
  deallocation_calls: Int,
  deallocation_bytes: Int,
  reallocation_calls: Int,
  reallocation_bytes: Int,
  alloc_calls_le16: Int,
  alloc_calls_le64: Int,
  alloc_calls_le256: Int,
  alloc_calls_le4096: Int,
  alloc_calls_gt4096: Int,
  peak_rss: Int,
}

type AllocationScenario = {name: Str, metrics: AllocationMetrics}

type Options = {
  scenario: Str,
  scale: Int,
  syscalls: Bool,
  keep_corpus: Bool,
  xsh: Path,
  xsht: Path,
  no_build: Bool,
  top_syscalls: Int,
}

let opts: Options = cli.parse(
  args,
  {
    scenario: {form: "--scenario NAME", default: "all"},
    scale: {form: "--scale N", default: 8},
    syscalls: {form: "--syscalls", default: false},
    keep_corpus: {form: "--keep-corpus", default: false},
    xsh: {form: "--xsh PATH", default: p"target/release/xsh"},
    xsht: {form: "--xsht PATH", default: p"target/release/xsht"},
    no_build: {form: "--no-build", default: false},
    top_syscalls: {form: "--top-syscalls N", default: 8},
  },
)?

let repo = fs.cwd()?
let stamp = run.text date +%Y%m%d-%H%M%S ?
let default_results = fp"${repo}/target/perf/${stamp.trim()}"
let results = fp"${env.get("XSH_PERF_RESULTS") ?? default_results.display()}"
let default_corpus = fp"${results}/corpus"
let corpus = fp"${env.get("XSH_PERF_CORPUS") ?? default_corpus.display()}"
let awk_ext = "NF > 1 {print tolower($NF)}"

pure value_after_equals(raw: Str) -> Result[Int] {
  let parts = raw.split("=")

  if parts.len() < 2 {
    return Err(ScriptError.Failed("perf-parse", f"missing key=value field: ${raw}"))
  }

  return parts[1].parse_int()?
}

pure metric_value(fields: List[Str], key: Str) -> Result[Int] {
  let prefix = f"${key}="

  for field in fields {
    if field.starts_with(prefix) {
      return value_after_equals(field)
    }
  }

  return Err(ScriptError.Failed("perf-parse", f"missing metric field: ${key}"))
}

proc parse_allocation_metrics(label: Str, stderr_text: Str) [error] -> Result[AllocationMetrics] {
  var metric_line = ""
  var sizes_line = ""

  for line in stderr_text.lines() {
    if line.starts_with("xsh perf:") {
      metric_line = line
    }

    if line.starts_with("xsh perf sizes:") {
      sizes_line = line
    }
  }

  if metric_line == "" {
    return Err(ScriptError.Failed("perf-parse", f"${label}: missing xsh perf line"))
  }

  if sizes_line == "" {
    return Err(ScriptError.Failed("perf-parse", f"${label}: missing xsh perf sizes line"))
  }

  let metric_fields = metric_line.fields()
  let size_fields = sizes_line.fields()

  if size_fields.len() < 8 {
    return Err(ScriptError.Failed("perf-parse", f"${label}: malformed xsh perf sizes line"))
  }

  return {
    allocation_calls: metric_value(metric_fields, "allocation_calls")?,
    allocation_bytes: metric_value(metric_fields, "allocation_bytes")?,
    deallocation_calls: metric_value(metric_fields, "deallocation_calls")?,
    deallocation_bytes: metric_value(metric_fields, "deallocation_bytes")?,
    reallocation_calls: metric_value(metric_fields, "reallocation_calls")?,
    reallocation_bytes: metric_value(metric_fields, "reallocation_bytes")?,
    alloc_calls_le16: value_after_equals(size_fields[3])?,
    alloc_calls_le64: value_after_equals(size_fields[4])?,
    alloc_calls_le256: value_after_equals(size_fields[5])?,
    alloc_calls_le4096: value_after_equals(size_fields[6])?,
    alloc_calls_gt4096: value_after_equals(size_fields[7])?,
    peak_rss: metric_value(metric_fields, "peak_rss")?,
  }
}

let newline = "\n"

pure normalized_count_lines(input_text: Str) -> List[Str] {
  input_text
    |> text.lines
    |> where .trim() != ""
    |> map { |line|
      let fields = line.fields()
      f"${fields[0]} ${fields[1]}"
    }
}

proc run_with_time(label: Str, target: Path, rest: List[Str]) [process, env, error] {
  let stdout = fp"${results}/${label}.stdout"
  let stderr = fp"${results}/${label}.stderr"
  let os = system.uname()?

  if os.sysname == "Darwin" {
    run XSH_PERF_ALLOC=1 /usr/bin/time -l $target @rest > $stdout 2> $stderr ?
    return
  }

  if os.sysname == "Linux" {
    run XSH_PERF_ALLOC=1 /usr/bin/time -v $target @rest > $stdout 2> $stderr ?
    return
  }

  run XSH_PERF_ALLOC=1 /usr/bin/time $target @rest > $stdout 2> $stderr ?
}

proc run_syscalls(label: Str, script: Path, rest: List[Str]) [process, error] {
  let trace = fp"${results}/${label}.syscalls"
  let stderr = fp"${results}/${label}.syscalls.stderr"
  run XSH_PERF_ALLOC=1 $opts.xsht trace --syscalls --trace-top-syscalls $opts.top_syscalls --trace-file $trace $script @rest > /dev/null 2> $stderr ?
}

proc run_fd_syscalls(label: Str) [fs, process, error] {
  let wrapper = fp"${results}/${label}.fd-syscalls.xsh"

  fs.write(
    wrapper,
    r"""let corpus = fp"${args[0]}"
let awk_ext = "NF > 1 {print tolower($NF)}"
cd corpus {
  run fd -tf | run awk -F. $awk_ext | run sort | run uniq -c | run sort -n ?
} ?
""",
  )?

  run_syscalls(label, wrapper, ["--", corpus.display()])?
}

proc run_xsh_scenario(name: Str) [process, env, error] {
  run_with_time(name, opts.xsh, [f"perf/scenarios/${name}.xsh", "--", corpus.display()])?

  if opts.syscalls {
    run_syscalls(name, fp"perf/scenarios/${name}.xsh", ["--", corpus.display()])?
  }
}

proc run_fd_extension_count() [fs, process, env, error] {
  match process.which("fd") {
    Ok(_) => {}
    Err(_) => {
      fs.write(
        fp"${results}/extension-count-fd.stderr",
        """fd not found; skipping fd extension-count comparison
""",
      )?

      return
    }
  }

  cd corpus {
    run fd -tf | run awk -F. $awk_ext | run sort | run uniq -c | run sort -n > fp"${results}/extension-count-fd.stdout" 2> fp"${results}/extension-count-fd.stderr"
  } ?

  if opts.syscalls {
    run_fd_syscalls("extension-count-fd")?
  }

  let xsh_lines = normalized_count_lines(fs.read_text(fp"${results}/extension-count.stdout")?)
  let fd_lines = normalized_count_lines(fs.read_text(fp"${results}/extension-count-fd.stdout")?)

  if xsh_lines != fd_lines {
    let diff_path = fp"${results}/extension-count.diff"

    fs.write(
      diff_path,
      f"""fd normalized:
${fd_lines.join()}

xsh normalized:
${xsh_lines.join()}
""",
    )?

    return Err(ScriptError.Failed("comparison", f"extension-count output differs; see ${diff_path.display()}"))
  }
}

proc print_syscall_tables() [fs, error] {
  let tables = fs.children(results)?
    |> where .kind == "file" and .name.ends_with(".syscalls")
    |> sort-by .name

  for entry in tables {
    let label = entry.name.replace(".syscalls", "")
    print f"${label} syscall summary:"
    var in_summary = false
    let lines = fs.read_text(entry.path)? |> text.lines

    for line in lines {
      if line == "syscall summary" {
        in_summary = true
        continue
      }

      if in_summary and line != "" {
        print f"  ${line}"
      }
    }
  }
}

proc write_allocation_json() [fs, error] {
  let stderr_files = fs.children(results)?
    |> where .kind == "file" and .name.ends_with(".stderr") and ! .name.ends_with("-fd.stderr") and ! .name.ends_with(
      ".syscalls.stderr",
    )
    |> sort-by .name

  var scenarios: List[AllocationScenario] = []

  for entry in stderr_files {
    let label = entry.name.replace(".stderr", "")
    let metrics = parse_allocation_metrics(label, fs.read_text(entry.path)?)?
    scenarios = scenarios.push({name: label, metrics})
  }

  json.write(
    fp"${results}/allocation.json",
    {
      version: 1,
      kind: "allocation-baseline",
      scale: opts.scale,
      build: "release",
      allocator: "libmimalloc-sys 0.1.47 secure",
      scenarios,
    },
    pretty: true,
  )?
}

if ! opts.no_build {
  run cargo build --release --features perf-metrics --bin xsh ?
  run cargo build --release -p xsht ?
}

fs.mkdir(results)?
run $opts.xsh perf/make-corpus.xsh -- --root $corpus --scale $opts.scale ?

let scenarios = [
  "extension-count",
  "manifest-hash",
  "json-log-rollup",
  "archive-package",
  "value-churn",
  "record-stream",
  "stream-heavy",
  "parse-check-heavy",
]

if opts.scenario == "all" {
  for name in scenarios {
    run_xsh_scenario(name)?
  }

  run_fd_extension_count()?
} else if scenarios.contains(opts.scenario) {
  run_xsh_scenario(opts.scenario)?

  if opts.scenario == "extension-count" {
    run_fd_extension_count()?
  }
} else {
  eprint f"unknown scenario: ${opts.scenario}"
  abort(2)
}

if ! opts.keep_corpus and (env.get("XSH_PERF_CORPUS") ?? "") == "" {
  fs.remove(corpus, missing_ok: true)?
}

write_allocation_json()?
let allocation_json = fp"${results}/allocation.json"
print f"results: ${results.display()}"
print f"allocation json: ${allocation_json.display()}"

let stderr_files = fs.children(results)?
  |> where .kind == "file" and .name.ends_with(".stderr")
  |> sort-by .name

for entry in stderr_files {
  let stderr_text = fs.read_text(entry.path)?
  let lines = stderr_text |> text.lines

  for line in lines {
    if line.starts_with("xsh perf:") {
      print $line
    }
  }
}

print_syscall_tables()?
