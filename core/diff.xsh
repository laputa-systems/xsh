#!/bin/xsh
use lib.gnu
use lib.diffutils

type Options = {unified: Bool, context: Str?, long_context: Str?, brief: Bool, report_same: Bool, text: Bool, label: Str?, help: Bool, version: Bool, files: List[Str]}

pure binary(data: Bytes) -> Bool {
  for index in range(data.len()) { if data.byte_at(index) == 0 { return true } }
  data.utf8() is Err(_)
}

pure labels(argv: List[Str]) -> List[Str] {
  var values: List[Str] = []
  var index = 0
  while index < argv.len() {
    let arg = argv[index]
    if arg == "--" { break }
    if arg.starts_with("--label=") { values += [arg.byte_slice(8)] } else if arg == "--label" or arg == "-L" { index += 1; values += [argv.get(index) ?? ""] } else if arg.starts_with("-L") { values += [arg.byte_slice(2)] }
    index += 1
  }
  values
}

proc main(...argv: List[Str]) [fs, io, process, env, error] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 2},
    unified: {form: "-u", default: false},
    context: {form: "-U LINES"},
    long_context: {form: "--unified[=LINES]", optional_default: "3"},
    brief: {form: "-q --brief", default: false},
    report_same: {form: "-s --report-identical-files", default: false},
    text: {form: "-a --text", default: false},
    label: {form: "-L --label LABEL"},
    ignore_case: {form: "-i --ignore-case", unsupported: true},
    ignore_space: {form: "-w --ignore-all-space", unsupported: true},
    ignore_space_change: {form: "-b --ignore-space-change", unsupported: true},
    ignore_blank: {form: "-B --ignore-blank-lines", unsupported: true},
    recursive: {form: "-r --recursive", unsupported: true},
    context_diff: {form: "-c --context[=LINES]", unsupported: true},
    ed: {form: "-e --ed", unsupported: true},
    side: {form: "-y --side-by-side", unsupported: true},
    color: {form: "--color[=WHEN]", unsupported: true},
    help: {form: "--help", default: false, stop: true},
    version: {form: "-v --version", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: diff [OPTION]... FILE1 FILE2\nCompare two files in normal or unified format. '-' reads stdin.\n-u, -U NUM, --unified[=NUM]  unified output with NUM context lines\n-q, --brief  report only whether files differ\n-s, --report-identical-files\n-a, --text  compare as UTF-8 text\n-L, --label LABEL  replace a displayed filename"); return }
  if opts.version { gnu.version("diff"); return }
  if opts.files.len() < 2 { gnu.missing_operand(2) }
  if opts.files.len() > 2 { gnu.extra_operand(opts.files[2], 2) }
  var context_text = opts.context ?? opts.long_context ?? "3"
  var argument = 0
  while argument < argv.len() {
    let word = argv[argument]
    if word == "--" { break }
    if word == "-U" { argument += 1; context_text = argv.get(argument) ?? "" } else if word.starts_with("-U") { context_text = word.byte_slice(2) } else if word.starts_with("--unified=") { context_text = word.byte_slice(10) } else if word == "--unified" { context_text = "3" }
    argument += 1
  }
  let context = context_text.parse_int() ?? -1
  if context < 0 { gnu.usage_error("invalid context length", 2) }
  let left_name = opts.files[0]
  let right_name = opts.files[1]
  if left_name == "-" and right_name == "-" { return }
  let first = if left_name == "-" { io.stdin_bytes() } else { fp"{left_name}".read_bytes() }
  let second = if right_name == "-" { io.stdin_bytes() } else { fp"{right_name}".read_bytes() }
  if let Err(failure) = first { gnu.name_error(left_name, failure); exit 2 }
  if let Err(failure) = second { gnu.name_error(right_name, failure); exit 2 }
  let left = first?
  let right = second?
  if left == right {
    if opts.report_same { gnu.write_text(f"Files {left_name} and {right_name} are identical\n") }
    return
  }
  if opts.brief { gnu.write_text(f"Files {left_name} and {right_name} differ\n"); exit 1 }
  if !opts.text and (binary(left) or binary(right)) { gnu.write_text(f"Binary files {left_name} and {right_name} differ\n"); exit 1 }
  let scratch = fs.tempdir()?
  defer scratch.close()
  scratch.write(p"original", left)
  scratch.write(p"modified", right)
  let root = scratch.host_path()?
  let result = diff.unified(fp"{root}/original", fp"{root}/modified", context: if opts.unified or opts.context != null or opts.long_context != null { context } else { 0 })
  if let Err(failure) = result { gnu.error(gnu.strerror(failure)); exit 2 }
  let changed = result?
  if opts.unified or opts.context != null or opts.long_context != null {
    let names = labels(argv)
    let first_label = names.get(0) ?? left_name
    let second_label = names.get(1) ?? right_name
    let lines = changed.text.lines().collect()
    gnu.write_text(f"--- {first_label}\n+++ {second_label}\n")
    for index in range(2, lines.len()) { gnu.write_text(lines[index] + "\n") }
  } else { gnu.write_text(diffutils.normal(changed.text)) }
  exit 1
}
