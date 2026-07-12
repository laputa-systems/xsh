type Options = {
  showcase: Str,
  syscalls: Bool,
  flamegraphs: Bool,
  no_build: Bool,
  repeat: Int,
  min_duration_ms: Int,
  top: Int,
  freq: Int,
  xsht: Path,
  xsh: Path,
}

let opts: Options = cli.parse(
  args,
  {
    showcase: {form: "--showcase NAME", default: "all"},
    syscalls: {form: "--syscalls", default: false},
    flamegraphs: {form: "--flamegraphs", default: false},
    no_build: {form: "--no-build", default: false},
    repeat: {form: "--repeat N", default: 1},
    min_duration_ms: {form: "--min-duration-ms N", default: 0},
    top: {form: "--top N", default: 20},
    freq: {form: "--freq N", default: 999},
    xsht: {form: "--xsht PATH", default: p"target/release/xsht"},
    xsh: {form: "--xsh PATH", default: p"target/release/xsh"},
  },
)?

let repo = fs.cwd()?
let stamp = run.text date +%Y%m%d-%H%M%S ?
let default_results = fp"${repo}/target/perf/showcase-tests-${stamp.trim()}"
let results = fp"${env.get("XSH_PERF_RESULTS") ?? default_results.display()}"
let xsht = opts.xsht.resolve()?
let xsh = opts.xsh.resolve()?
let repeat_runner = fp"${repo}/perf/repeat-tests.xsh"

pure artifact(label: Str, ext: Str) -> Path {
  return fp"${results}/${label}.${ext}"
}

proc run_with_time(label: Str, filter: Str) [fs, process, env, error] {
  let stdout = artifact(label, "stdout")
  let stderr = artifact(label, "stderr")
  let os = system.uname()?

  cd repo {
    if os.sysname == "Darwin" {
      run XSH_PERF_ALLOC=1 /usr/bin/time -l $xsh $repeat_runner -- --xsht $xsht --filter $filter --repeat $opts.repeat --min-duration-ms $opts.min_duration_ms > $stdout 2> $stderr ?
      return
    }

    if os.sysname == "Linux" {
      run XSH_PERF_ALLOC=1 /usr/bin/time -v $xsh $repeat_runner -- --xsht $xsht --filter $filter --repeat $opts.repeat --min-duration-ms $opts.min_duration_ms > $stdout 2> $stderr ?
      return
    }

    run XSH_PERF_ALLOC=1 /usr/bin/time $xsh $repeat_runner -- --xsht $xsht --filter $filter --repeat $opts.repeat --min-duration-ms $opts.min_duration_ms > $stdout 2> $stderr ?
  } ?
}

proc run_strace(label: Str, filter: Str) [fs, process, env, error] {
  match process.which("strace") {
    Ok(_) => {}
    Err(_) => {
      fs.write(
        artifact(label, "strace"),
        """strace not found; syscall summary skipped
""",
      )?

      return
    }
  }

  let stdout = artifact(label, "strace.stdout")
  let stderr = artifact(label, "strace.stderr")
  let summary = artifact(label, "strace")

  cd repo {
    run strace -f -c -o $summary $xsh $repeat_runner -- --xsht $xsht --filter $filter --repeat $opts.repeat --min-duration-ms $opts.min_duration_ms > $stdout 2> $stderr ?
  } ?
}

proc run_flamegraph(label: Str, filter: Str) [fs, process, env, error] {
  match process.which("perf") {
    Ok(_) => {}
    Err(_) => {
      fs.write(
        artifact(label, "perf.stderr"),
        """perf not found; flamegraph skipped
""",
      )?

      return
    }
  }

  let data = artifact(label, "perf.data")
  let script = artifact(label, "perf.script")
  let folded = artifact(label, "folded")
  let svg = artifact(label, "svg")
  let top = artifact(label, "top")
  let stdout = artifact(label, "perf.stdout")
  let stderr = artifact(label, "perf.stderr")

  cd repo {
    run perf record -F $opts.freq -g -o $data -- $xsh $repeat_runner -- --xsht $xsht --filter $filter --repeat $opts.repeat --min-duration-ms $opts.min_duration_ms > $stdout 2> $stderr ?
  } ?

  run perf script --demangle -i $data > $script ?
  run $xsh showcase/perf-collapse.xsh -- $script > $folded ?
  run $xsh showcase/flamegraph.xsh -- $folded > $svg ?
  run $xsh showcase/perf-collapse.xsh -- --top $opts.top $script > $top ?
}

proc print_time_summary(label: Str) [fs, error] {
  let stderr = artifact(label, "stderr")

  if ! stderr.exists()? {
    return
  }

  let repeated = opts.repeat > 1 or opts.min_duration_ms > 0
  var saw_repeat = false

  for line in stderr.lines()? {
    if line.starts_with("xsh perf repeat:") {
      print f"  ${line}"
      saw_repeat = true
    }

    if line.starts_with("xsh perf:") or line.starts_with("xsh perf sizes:") {
      if ! repeated or saw_repeat {
        print f"  ${line}"
      }
    }

    if line.split("Maximum resident set size").len() > 1 or line.split("maximum resident set size").len() > 1 {
      print f"  ${line.trim()}"
    }

    if line.split("Elapsed (wall clock) time").len() > 1 or line.split("real ").len() > 1 {
      print f"  ${line.trim()}"
    }
  }
}

proc print_strace_summary(label: Str) [fs, error] {
  let trace_path = artifact(label, "strace")

  if ! trace_path.exists()? {
    return
  }

  let lines = trace_path.lines()? |> where .trim() != ""

  if lines.len() == 0 or lines[0].starts_with("strace not found") {
    print f"  ${lines.get(0, "strace summary unavailable")}"
    return
  }

  print "  syscall summary:"

  for line in lines |> take(8) {
    print f"    ${line}"
  }
}

if ! opts.no_build {
  run cargo build --release --features perf-metrics --bin xsh ?
  run cargo build --release -p xsht ?
}

if opts.repeat < 1 {
  eprint "--repeat must be at least 1"
  abort(2)
}

if opts.min_duration_ms < 0 {
  eprint "--min-duration-ms must be non-negative"
  abort(2)
}

fs.mkdir(results.parent)?
fs.mkdir(results)?

let showcases = fs.files(p"showcase")
  |> where .path.parent().name() == "showcase"
  |> where .path.ext() == "xsh"
  |> sort-by .name

let selected = showcases |> where opts.showcase == "all" or .path.with_ext("").name() == opts.showcase

if selected.len() == 0 {
  eprint f"unknown showcase: ${opts.showcase}"
  abort(2)
}

for entry in selected {
  let label = entry.path.with_ext("").name()
  let filter = f"showcase/tests/test-${label}.xsh"

  if ! fp"${filter}".exists()? {
    eprint f"missing showcase test: ${filter}"
    abort(2)
  }

  print f"${label}:"
  run_with_time(label, filter)?
  print_time_summary(label)?

  if opts.syscalls {
    run_strace(label, filter)?
    print_strace_summary(label)?
  }

  if opts.flamegraphs {
    run_flamegraph(label, filter)?
    print f"  flamegraph: ${artifact(label, "svg").display()}"
  }
}

print f"results: ${results.display()}"
