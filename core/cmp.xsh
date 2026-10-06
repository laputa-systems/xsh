#!/bin/xsh
use lib.gnu
use lib.diffutils

type Options = {silent: Bool, list: Bool, print_bytes: Bool, skip: Str, limit: Str?, help: Bool, version: Bool, files: List[Str]}

proc main(...argv: List[Str]) [fs, io, process, env, error] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 2},
    silent: {form: "-s --silent --quiet", default: false},
    list: {form: "-l --verbose", default: false},
    print_bytes: {form: "-b --print-bytes", default: false},
    skip: {form: "-i --ignore-initial SKIP", default: "0"},
    limit: {form: "-n --bytes LIMIT"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "-v --version", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: cmp [OPTION]... FILE1 [FILE2 [SKIP1 [SKIP2]]]\nCompare bytes. FILE2 defaults to stdin.\n-s, --silent  status only\n-l, --verbose  list differing bytes in octal\n-b, --print-bytes  include byte values\n-i, --ignore-initial SKIP1[:SKIP2]\n-n, --bytes LIMIT"); return }
  if opts.version { gnu.version("cmp"); return }
  if opts.files.is_empty() { gnu.missing_operand(2) }
  if opts.files.len() > 4 { gnu.extra_operand(opts.files[4], 2) }
  if opts.silent and (opts.list or opts.print_bytes) { gnu.usage_error("options -l and -b are incompatible with -s", 2) }
  let left = opts.files[0]
  let right = opts.files.get(1) ?? "-"
  if left == "-" and right == "-" { return }
  let skips = opts.skip.split(":")
  if skips.len() > 2 { gnu.usage_error("invalid --ignore-initial argument", 2) }
  let first_skip = opts.files.get(2) ?? skips[0]
  let second_skip = opts.files.get(3) ?? skips.get(1) ?? skips[0]
  let skip_left = diffutils.byte_count(first_skip)
  let skip_right = diffutils.byte_count(second_skip)
  let limit = if let text = opts.limit { diffutils.byte_count(text) } else { 9223372036854775807 }
  if skip_left == null or skip_right == null or limit == null { gnu.usage_error("invalid byte count", 2) }
  let first = skip_left ?? 0
  let second = skip_right ?? 0
  let maximum = limit ?? 0
  for operand in [left, right] {
    if operand == "-" { continue }
    let checked = diffutils.read_chunk(operand, 0, 0)
    if let Err(failure) = checked { if !opts.silent { gnu.name_error(operand, failure) }; exit 2 }
  }
  if left == "-" { diffutils.skip_stdin(first)? }
  if right == "-" { diffutils.skip_stdin(second)? }
  var offset = 0
  var line = 1
  var different = false
  while offset < maximum {
    let count = if maximum - offset < 65536 { maximum - offset } else { 65536 }
    let left_result = diffutils.read_chunk(left, first + offset, count)
    let right_result = diffutils.read_chunk(right, second + offset, count)
    if let Err(failure) = left_result { if !opts.silent { gnu.name_error(left, failure) }; exit 2 }
    if let Err(failure) = right_result { if !opts.silent { gnu.name_error(right, failure) }; exit 2 }
    let a = left_result?
    let b = right_result?
    let shared = if a.len() < b.len() { a.len() } else { b.len() }
    if opts.list {
      for index in range(shared) {
        let av = a.byte_at(index) ?? 0
        let bv = b.byte_at(index) ?? 0
        if av != bv {
          different = true
          let values = if opts.print_bytes { f"{diffutils.octal(av)} {diffutils.display_byte(av)} {diffutils.octal(bv)} {diffutils.display_byte(bv)}" } else { f"{diffutils.octal(av)} {diffutils.octal(bv)}" }
          gnu.write_text(f"{diffutils.pad(f"{offset + index + 1}", 6)} {values}\n")
        }
      }
    } else {
      let compared = a[0..shared].compare(b[0..shared])
      if !compared.equal {
        if !opts.silent {
          let detail = if opts.print_bytes { f" is {diffutils.octal(compared.left)} {diffutils.display_byte(compared.left)} {diffutils.octal(compared.right)} {diffutils.display_byte(compared.right)}" } else { "" }
          gnu.write_text(f"{left} {right} differ: byte {offset + compared.byte}, line {line + compared.line - 1}{detail}\n")
        }
        exit 1
      }
    }
    if a.len() != b.len() {
      if !opts.silent { gnu.error(f"EOF on {gnu.quote(if a.len() < b.len() { left } else { right })} after byte {offset + shared}, in line {line + diffutils.newline_count(a[0..shared])}") }
      exit 1
    }
    if shared == 0 { break }
    line += diffutils.newline_count(a)
    offset += shared
  }
  if different { exit 1 }
}
