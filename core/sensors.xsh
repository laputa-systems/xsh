#!/bin/xsh
use lib.gnu
use lib.hardware
use lib.system_report as report

type Options = {fahrenheit: Bool, raw: Bool, json: Bool, no_adapter: Bool, allow_empty: Bool, help: Bool, version: Bool, chips: List[Str]}

proc inventory(opts: Options) -> Result[Unit] {
  let section = hardware.sensors_live()?
  if ! section.status.enumeration_succeeded and section.status.state != report.SectionAbsent { gnu.error("cannot enumerate hardware sensors"); exit 1 }
  var channels = section.channels
  if ! opts.chips.is_empty() {
    channels = channels |> where { |channel| opts.chips |> any { |pattern| hardware.chip_matches(hardware.sensor_chip_name(channel), pattern)? } }
  }
  if channels.is_empty() and ! opts.allow_empty { gnu.error("No sensors found!"); exit 1 }
  if opts.json {
    if opts.fahrenheit or opts.no_adapter { gnu.usage_error("JSON sensor output uses native units and does not include adapter labels") }
    gnu.write_text(hardware.sensor_json(channels)? + "\n")
  } else {
    for line in hardware.sensor_lines(channels, opts.fahrenheit, opts.raw, opts.no_adapter) {
      print $line
    }
  }
}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1, unsupported: {"-s": "configured sensor writes are not available", "--set": "configured sensor writes are not available", "-c": "libsensors configuration evaluation is not available", "--config-file": "libsensors configuration evaluation is not available", "--bus-list": "stable sensor bus identities are not available"}},
    fahrenheit: {form: "-f --fahrenheit", default: false}, raw: {form: "-u", default: false}, json: {form: "-j", default: false},
    no_adapter: {form: "-A --no-adapter", default: false}, allow_empty: {form: "-n --allow-no-sensors", default: false}, help: {form: "-h --help", default: false, stop: true}, version: {form: "-v --version", default: false, stop: true}, chips: {form: "...CHIP"},
  })?
  if opts.help { gnu.help("Usage: sensors [-f] [-u|-j] [-A] [-n] [CHIP...]\nDisplay hardware sensor channels and limits."); return }
  if opts.version { gnu.version("sensors"); return }
  if opts.raw and opts.fahrenheit { gnu.usage_error("raw sensor output uses native units; -u and -f are incompatible") }
  if opts.raw and opts.json { gnu.usage_error("options -u and -j are mutually exclusive") }
  for selector in opts.chips {
    if let Err(failure) = hardware.chip_matches("", selector) { gnu.usage_error(failure.message) }
  }
  if let Err(failure) = inventory(opts) { gnu.error(failure.message); exit 1 }
}
