#!/bin/xsh
use lib.gnu
use lib.memory_report as report

const HELP = "\nUsage:\n lsmem [options]\n\nList the ranges of available memory with their online status.\n\nOptions:\n -J, --json           use JSON output format\n -P, --pairs          use key=\"value\" output format\n -a, --all            list each individual memory block\n -b, --bytes          print SIZE in bytes rather than in human readable format\n -n, --noheadings     don't print headings\n -o, --output <list>  output columns\n     --output-all     output all columns\n -r, --raw            use raw output format\n -S, --split <list>   split ranges by specified columns\n -s, --sysroot <dir>  use the specified directory as system root\n     --summary[=when] print summary information (never,always or only)\n\n -h, --help           display this help\n -V, --version        display version\n\nAvailable output columns:\n      RANGE  start and end address of the memory range\n       SIZE  size of the memory range\n      STATE  online status of the memory range\n  REMOVABLE  memory is removable\n      BLOCK  memory block number or blocks range\n       NODE  numa node of memory\n      ZONES  valid zones for the memory range\n CONFIGURED  configuration status of the memory range\n MEMMAP-ON-MEMORY  memmap-on-memory status of the memory range\n\nFor more details see lsmem(1)."

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: report.Options = cli.applet(argv, {
    gnu: {status: 1},
    all: {form: "-a --all", default: false},
    bytes: {form: "-b --bytes", default: false},
    json: {form: "-J --json", default: false},
    noheadings: {form: "-n --noheadings", default: false},
    output: {form: "-o --output LIST"},
    output_all: {form: "--output-all", default: false},
    pairs: {form: "-P --pairs", default: false},
    raw: {form: "-r --raw", default: false},
    split: {form: "-S --split LIST"},
    sysroot: {form: "-s --sysroot DIR", kind: "Path"},
    summary: {form: "--summary[=WHEN]", optional_default: "only"},
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    operands: {form: "...ARG"},
  })?
  if opts.help { gnu.help(HELP); return }
  if opts.version { gnu.version("lsmem"); return }
  report.check_exclusive(argv)
  if ! opts.operands.is_empty() { gnu.usage_error("bad usage") }
  let modes = report.modes(argv)
  if !modes.table and !modes.summary {
    gnu.error("options --{raw,json,pairs} and --summary=only are mutually exclusive")
    exit 1
  }

  var selected: List[Str] = []
  var split: List[Str] = []
  if modes.table {
    let environment_columns: Str? = if opts.output == null { if let Ok(value) = env.get("LSMEM_COLUMNS") { value } else { null } } else { null }
    selected = report.columns(opts, environment_columns)
    split = report.split_columns(opts, selected)
  }

  let root_path = if opts.sysroot == null { p"/" } else { opts.sysroot ?? p"/" }
  let root = fs.open_root(root_path)?
  defer root.close()
  let inventory = report.collect(root, split, opts.all, modes.summary)
  gnu.write_text(report.render(inventory, opts, selected, modes))
}
