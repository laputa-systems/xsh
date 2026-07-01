type Options = {syscalls: Bool, xsh: Path, no_build: Bool, top_syscalls: Int}

let opts: Options = cli.parse(
  args,
  {
    syscalls: {form: "--syscalls", default: false},
    xsh: {form: "--xsh PATH", default: p"target/release/xsh"},
    no_build: {form: "--no-build", default: false},
    top_syscalls: {form: "--top-syscalls N", default: 12},
  },
)?

let repo = fs.cwd()?
let stamp = run.text date +%Y%m%d-%H%M%S ?
let default_results = fp"${repo}/target/perf/extension-count-example-${stamp.trim()}"
let results = fp"${env.get("XSH_PERF_RESULTS") ?? default_results.display()}"

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

proc run_syscalls(label: Str, target: Path, rest: List[Str]) [process, error] {
  let trace = fp"${results}/${label}.syscalls"
  let stderr = fp"${results}/${label}.syscalls.stderr"
  run XSH_PERF_ALLOC=1 $target --trace --syscalls --trace-top-syscalls $opts.top_syscalls --trace-file $trace @rest > /dev/null 2> $stderr ?
}

proc run_fd_syscalls(label: Str) [fs, process, error] {
  let wrapper = fp"${results}/${label}.fd-syscalls.xsh"

  fs.write(
    wrapper,
    r"""let root = fp"${args[0]}"
let awk_ext = "NF > 1 {print tolower($NF)}"
cd root {
  run fd -tf | run awk -F. $awk_ext | run sort | run uniq -c | run sort -n ?
} ?
""",
  )?

  run_syscalls(label, opts.xsh, [wrapper.display(), "--", repo.display()])?
}

proc print_time_summary(label: Str) [fs, error] {
  let stderr = fs.read_text(fp"${results}/${label}.stderr")?
  let lines = stderr |> text.lines

  for line in lines {
    let fields = line.fields()

    if "Maximum resident set size" in line or "maximum resident set size" in line {
      let rss = if fields[0] == "Maximum" or fields[0] == "maximum" { fields[fields.len() - 1] } else { fields[0] }
      print f"  max_rss_kb=${rss}"
    }

    if line.starts_with("xsh perf:") {
      let alloc_line = line.replace("xsh perf: ", "")
      print f"  ${alloc_line}"
    }
  }
}

proc print_syscall_summary(label: Str) [fs, error] {
  let syscall_path = fp"${results}/${label}.syscalls"

  if ! fs.exists(syscall_path)? {
    return
  }

  let lines = fs.read_text(syscall_path)? |> text.lines
  var in_summary = false

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

proc print_summary(label: Str) [fs, error] {
  print $label
  print_time_summary(label)?
  print_syscall_summary(label)?
}

if ! opts.no_build {
  run cargo build --release --features perf-metrics --bin xsh ?
}

match process.which("fd") {
  Ok(_) => {}
  Err(_) => {
    eprint "fd not found"
    abort(2)
  }
}

fs.mkdir(results)?
run_with_time("extension-count-example", opts.xsh, ["examples/extension-count.xsh"])?

run_with_time(
  "extension-count-fd",
  p"sh",
  [
    "-c",
    "cd \"$1\" && fd -tf | awk -F. 'NF > 1 {print tolower($NF)}' | sort | uniq -c | sort -n",
    "xsh-fd",
    repo.display(),
  ],
)?

let xsh_lines = normalized_count_lines(fs.read_text(fp"${results}/extension-count-example.stdout")?)
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

  eprint f"extension-count output differs; see ${diff_path.display()}"
  abort(1)
}

if opts.syscalls {
  run_syscalls("extension-count-example", opts.xsh, ["examples/extension-count.xsh"])?
  run_fd_syscalls("extension-count-fd")?
}

print f"results: ${results.display()}"
print_summary("extension-count-example")?
print_summary("extension-count-fd")?
