type Options = {root: Path, scale: Int}

let opts: Options = cli.parse(
  args,
  {root: {form: "--root PATH", kind: "Path", required: true}, scale: {form: "--scale N", default: 8}},
)?

if opts.scale < 1 {
  eprint "scale must be a positive integer"
  abort(2)
}

let root = opts.root
fs.remove(root, missing_ok: true)?
fs.mkdir(fp"${root}/src/app")?
fs.mkdir(fp"${root}/src/lib")?
fs.mkdir(fp"${root}/docs")?
fs.mkdir(fp"${root}/tests/fixtures")?
fs.mkdir(fp"${root}/logs")?
fs.mkdir(fp"${root}/pkgroot/etc/demo")?
fs.mkdir(fp"${root}/pkgroot/usr/bin")?
fs.mkdir(fp"${root}/pkgroot/usr/lib/demo")?
fs.mkdir(fp"${root}/pkgroot/usr/share/demo")?
fs.mkdir(fp"${root}/.cache")?

fs.write(
  fp"${root}/.cache/ignored.log",
  """ignored
""",
)?

fs.write(
  fp"${root}/pkgroot/etc/demo/config.toml",
  """name = "demo"
version = "1.0.0"
jobs = 4
""",
)?

let democtl = fp"${root}/pkgroot/usr/bin/democtl"
let dollar = "$"

fs.write(
  democtl,
  """#!/bin/sh
printf "democtl %s\\n" "$1"
""",
)?

fs.chmod(democtl, 0o755)?
let payload = fp"${root}/pkgroot/usr/share/demo/payload.txt"
var payload_lines: List[Str] = []
let services = ["api", "worker", "scheduler"]
var i = 0

while i < opts.scale {
  fs.write(
    fp"${root}/src/app/task_${i}.xsh",
    f"""proc task_${i}(target: Path) -> Result[Unit] {
  print ${dollar}{target.display()}
}
""",
  )?

  fs.write(
    fp"${root}/src/lib/module_${i}.rs",
    f"""pub fn module_${i}() -> usize {
    ${i}
}
""",
  )?

  fs.write(
    fp"${root}/docs/chapter-${i}.md",
    f"""# Chapter ${i}

Package workflow notes for demo module ${i}.
""",
  )?

  fs.write(
    fp"${root}/tests/fixtures/case_${i}.xsh",
    f"""let fixture = p"src/app/task_${i}.xsh"
print ${dollar}{fixture.ext}
""",
  )?

  fs.write(
    fp"${root}/pkgroot/usr/share/demo/artifact_${i}.txt",
    f"""artifact ${i}
""",
  )?

  fs.write(
    fp"${root}/pkgroot/usr/lib/demo/libdemo_${i}.so",
    f"""library payload ${i}
""",
  )?

  payload_lines = payload_lines.push(f"""payload line ${i}: src/app/task_${i}.xsh -> usr/share/demo/artifact_${i}.txt
""")

  var service_index = 0
  var day_lines: List[Str] = []

  for service in services {
    var event = 0

    while event < 8 {
      let level_index = (i + service_index + event) % 4
      var level = "debug"

      if level_index == 0 {
        level = "info"
      }

      if level_index == 1 {
        level = "warn"
      }

      if level_index == 2 {
        level = "error"
      }

      let duration = 10 + i * 3 + service_index * 5 + event

      day_lines = day_lines.push(
        f"""{"service":"${service}","level":"${level}","duration_ms":${duration},"message":"processed package ${i} event ${event}"}
""",
      )

      event += 1
    }

    service_index += 1
  }

  fs.write(fp"${root}/logs/day-${i}.jsonl", day_lines.join())?
  i += 1
}

fs.write(payload, payload_lines.join())?
