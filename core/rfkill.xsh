#!/bin/xsh
use lib.gnu
use lib.hardware

type Options = {json: Bool, raw: Bool, noheadings: Bool, output: Str?, output_all: Bool, help: Bool, version: Bool, operands: List[Str]}

proc inventory(opts: Options) -> Result[Unit] {
  let command = if opts.operands.is_empty() { "table" } else { opts.operands[0] }
  let selectors = opts.operands |> drop(1)
  if command not in ["table", "list", "block", "unblock"] { gnu.usage_error(f"unsupported rfkill command {gnu.quote(command)}") }
  if opts.output_all and opts.output != null { gnu.usage_error("options --output and --output-all are mutually exclusive") }
  let columns = if opts.output_all { ["ID", "TYPE", "TYPE-DESC", "DEVICE", "SOFT", "HARD"] } else { hardware.radio_columns(opts.output ?? "ID,TYPE,DEVICE,SOFT,HARD")? }
  if opts.json and opts.raw { gnu.usage_error("options --json and --raw are mutually exclusive") }
  if command == "block" or command == "unblock" {
    if selectors.is_empty() { gnu.missing_operand_after(command) }
    if opts.json or opts.raw or opts.noheadings or opts.output != null or opts.output_all { gnu.usage_error("output options require a listing command") }
  }
  let devices = linux.rfkill_list()?.collect() |> sort-by .id
  let selected = hardware.rfkill_select(devices, selectors)?
  if command == "block" or command == "unblock" {
    hardware.rfkill_set(devices, selectors, command == "block")?
    return
  }
  if command == "list" and ! opts.json and ! opts.raw and opts.output == null and ! opts.output_all and ! opts.noheadings {
    for line in hardware.rfkill_list_lines(selected) { print $line }
    return
  }
  if opts.json { gnu.write_text(hardware.rfkill_json(selected, columns)? + "\n") } else { for line in hardware.rfkill_table_lines(selected, columns, ! opts.noheadings, opts.raw)? { print $line } }
}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    json: {form: "-J --json", default: false}, raw: {form: "-r --raw", default: false}, noheadings: {form: "-n --noheadings", default: false},
    output: {form: "-o --output LIST"}, output_all: {form: "--output-all", default: false},
    help: {form: "-h --help", default: false, stop: true}, version: {form: "-V --version", default: false, stop: true}, operands: {form: "...COMMAND"},
  })?
  if opts.help { gnu.help("Usage: rfkill [OPTIONS] [list [ID|TYPE]... | block ID|TYPE... | unblock ID|TYPE...]\nList radio state or set the explicitly selected software block state."); return }
  if opts.version { gnu.version("rfkill"); return }
  if let Err(failure) = inventory(opts) { gnu.error(failure.message); exit 1 }
}
