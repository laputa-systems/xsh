#!/usr/bin/env -S xsh --
# Hyperfine-shaped command-line benchmarking tool.
# Usage: xsh showcase/hyperfine.xsh -- [OPTIONS] COMMAND [COMMAND ...]
#   --warmup N           runs to execute (and discard) before timing (default 0)
#   --runs N             timed runs per command (default 10)
#   --shell S            wrap each command in `S -c "<command>"` (for pipes/globs);
#                        default is direct execution through xsh's own launcher
#   --subtract-startup   subtract the measured `xsh --startup` cost from each run
#                        (use when benchmarking xsh scripts, to isolate their work)
#   --ignore-failure     don't warn when a command exits non-zero
#   --export-json F      write hyperfine-shaped JSON results to F
# Example: xsh showcase/hyperfine.xsh -- --warmup 2 --runs 10 'sleep 0.1' 'sleep 0.05'
#
# A port to diversify XSH's proving grounds onto the *effectful* axis: subprocess
# spawning, wall-clock + CPU timing, warmups, float statistics, and serialization.
# See PORTS.md. Commands run directly through xsh's own process launcher (no /bin/sh)
# via `time.measure(.., quiet: true)`, which was extended (for this port) to report
# nanosecond wall time plus user/system CPU, and to discard child output. xsh's fixed
# startup cost is probed via `xsh --startup` and reported as a calibration baseline.
type Opts = {
  warmup: Int,
  runs: Int,
  shell: Str,
  subtract_startup: Bool,
  ignore_failure: Bool,
  export_json: Str,
  commands: List[Str],
}

type Summary = {
  name: Str,
  mean_ms: Float,
  stddev_ms: Float,
  median_ms: Float,
  min_ms: Float,
  max_ms: Float,
  user_ms: Float,
  system_ms: Float,
  times_ms: List[Float],
}

type Baseline = {wall_ns: Int, user_ns: Int, system_ns: Int}

pure ns_to_ms(ns: Int) -> Float {
  return ns.float() / 1000000.0
}

pure floor0(n: Int) -> Int {
  if n < 0 {
    return 0
  }

  return n
}

pure mean_ms_of(times_ns: List[Int]) -> Float {
  return ns_to_ms(times_ns |> sum) / times_ns.len().float()
}

# Sample standard deviation in milliseconds (divides by n-1, matching hyperfine).
pure stddev_ms_of(times_ns: List[Int], mean_ms: Float) -> Float {
  let n = times_ns.len()

  if n <= 1 {
    return 0.0
  }

  var acc = 0.0

  for t in times_ns {
    let delta = ns_to_ms(t) - mean_ms
    acc += delta * delta
  }

  return (acc / (n - 1).float()).sqrt()
}

pure median_ms_of(sorted_ns: List[Int]) -> Float {
  let n = sorted_ns.len()

  if n % 2 == 0 {
    let lo = ns_to_ms(sorted_ns.get(n / 2 - 1, 0))
    let hi = ns_to_ms(sorted_ns.get(n / 2, 0))
    return (lo + hi) / 2.0
  }

  return ns_to_ms(sorted_ns.get(n / 2, 0))
}

proc build_command(text: Str, opts: Opts) [] -> Command {
  if opts.shell != "" {
    return process.command_argv(opts.shell, [opts.shell, "-c", text])
  }

  # Direct execution: tokenize on whitespace and run argv through xsh's launcher.
  let argv = text.fields()
  return process.command_argv(argv.get(0, text), argv)
}

# Mean cost of `xsh --startup` (boot the interpreter and exit), used as a calibration
# baseline: reported always, and subtracted per-run under --subtract-startup.
proc xsh_startup_baseline() [process, time, error] -> Result[Baseline] {
  let exe = applet.current_exe()?
  let probe = process.command_argv(exe.display(), [exe.display(), "--startup"])

  for _ in range(3) {
    time.measure(probe, quiet: true)?
  }

  var wall_total = 0
  var user_total = 0
  var system_total = 0
  let n = 10

  for _ in range(n) {
    let result = time.measure(probe, quiet: true)?
    wall_total += result.wall_ns
    user_total += result.user_ns
    system_total += result.system_ns
  }

  return {wall_ns: wall_total / n, user_ns: user_total / n, system_ns: system_total / n}
}

proc bench(text: Str, opts: Opts, baseline: Baseline) [time, error] -> Result[Summary] {
  let command = build_command(text, opts)

  for _ in range(opts.warmup) {
    time.measure(command, quiet: true)?
  }

  var times_ns: List[Int] = []
  var user_total = 0
  var system_total = 0
  var failures = 0

  for _ in range(opts.runs) {
    let result = time.measure(command, quiet: true)?

    if ! result.status.exited_with(0) {
      failures += 1
    }

    times_ns = times_ns.push(floor0(result.wall_ns - baseline.wall_ns))
    user_total += floor0(result.user_ns - baseline.user_ns)
    system_total += floor0(result.system_ns - baseline.system_ns)
  }

  if failures > 0 and ! opts.ignore_failure {
    print f"  Warning: command exited non-zero on ${failures} of ${opts.runs} runs."
  }

  let n = times_ns.len()
  let sorted = times_ns |> sort
  let mean = mean_ms_of(times_ns)
  var times_ms = [ns_to_ms(t) for t in times_ns]

  return {
    name: text,
    mean_ms: mean,
    stddev_ms: stddev_ms_of(times_ns, mean),
    median_ms: median_ms_of(sorted),
    min_ms: ns_to_ms((times_ns |> min)?),
    max_ms: ns_to_ms((times_ns |> max)?),
    user_ms: ns_to_ms(user_total) / n.float(),
    system_ms: ns_to_ms(system_total) / n.float(),
    times_ms,
  }
}

proc report(summary: Summary, runs: Int) [io] {
  print f"Benchmark: ${summary.name}"

  print f"  Time (mean ± σ):  ${summary.mean_ms.format(1)} ms ± ${summary.stddev_ms.format(1)} ms    [User: ${summary.user_ms.format(
    1,
  )} ms, System: ${summary.system_ms.format(1)} ms]"

  print f"  Range (min … max):  ${summary.min_ms.format(1)} ms … ${summary.max_ms.format(1)} ms    ${runs} runs"
  print ""
}

proc print_summary(results: List[Summary]) [error, io] {
  var fastest = results.get(0)?

  for result in results {
    if result.mean_ms < fastest.mean_ms {
      fastest = result
    }
  }

  print "Summary"
  print f"  '${fastest.name}' ran"

  for result in results {
    if result.name != fastest.name {
      let ratio = result.mean_ms / fastest.mean_ms

      # Uncertainty propagation for a ratio of independent means.
      let r_rel = result.stddev_ms / result.mean_ms
      let f_rel = fastest.stddev_ms / fastest.mean_ms
      let ratio_sd = ratio * (r_rel * r_rel + f_rel * f_rel).sqrt()
      print f"  ${ratio.format(2)} ± ${ratio_sd.format(2)} times faster than '${result.name}'"
    }
  }
}

# hyperfine's JSON shape: {"results": [{command, mean, stddev, median, user, system,
# min, max, times}, ...]} with all times in seconds.
proc export_json(results: List[Summary], dest: Str) [fs, error] {
  var entries: List[Any] = []

  for result in results {
    var times_s = [t / 1000.0 for t in result.times_ms]

    entries = entries.push({
      command: result.name,
      mean: result.mean_ms / 1000.0,
      stddev: result.stddev_ms / 1000.0,
      median: result.median_ms / 1000.0,
      user: result.user_ms / 1000.0,
      system: result.system_ms / 1000.0,
      min: result.min_ms / 1000.0,
      max: result.max_ms / 1000.0,
      times: times_s,
    })
  }

  let encoded = json.encode({results: entries}, pretty: true)?
  fp"${dest}".write(encoded)?
}

proc main(...argv: List[Str]) [fs, process, time, error, io] {
  let opts: Opts = cli.parse(
    argv,
    {
      warmup: {form: "--warmup N", kind: "UInt", default: 0},
      runs: {form: "--runs N", kind: "UInt", default: 10, min: 1},
      shell: {form: "--shell S", default: ""},
      subtract_startup: {form: "--subtract-startup", default: false},
      ignore_failure: {form: "--ignore-failure", default: false},
      export_json: {form: "--export-json FILE", default: ""},
      commands: {form: "...COMMAND", repeated: true, required: true},
    },
  )?

  let startup = xsh_startup_baseline()?
  print f"xsh interpreter startup (--startup): ${ns_to_ms(startup.wall_ns).format(1)} ms"

  if opts.subtract_startup {
    print "  (subtracting xsh startup from every run)"
  }

  let baseline = if opts.subtract_startup { startup } else { {wall_ns: 0, user_ns: 0, system_ns: 0} }
  print ""
  var results: List[Summary] = []

  for command_text in opts.commands {
    let summary = bench(command_text, opts, baseline)?
    report(summary, opts.runs)
    results = results.push(summary)
  }

  if results.len() > 1 {
    print_summary(results)?
  }

  if opts.export_json != "" {
    export_json(results, opts.export_json)?
  }
}
