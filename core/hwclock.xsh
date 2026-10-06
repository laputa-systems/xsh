#!/bin/xsh
use lib.gnu
use lib.date_parse
use lib.system_control as control

proc main(...argv: List[Str]) {
  let opts = cli.applet(argv, {
    gnu: {unsupported: {"-a": "hardware clock drift adjustment is not available", "--adjust": "hardware clock drift adjustment is not available", "--get": "hardware clock drift correction is not available", "-l": "local-time RTC interpretation is not available", "--localtime": "local-time RTC interpretation is not available", "-f": "selecting an RTC device is not available", "--rtc": "selecting an RTC device is not available", "--adjfile": "hardware clock drift files are not available", "--predict": "hardware clock drift prediction is not available", "--systz": "kernel timezone updates are not available", "--update-drift": "hardware clock drift updates are not available"}},
    show: {form: "-r --show", default: false}, hctosys: {form: "-s --hctosys", default: false}, systohc: {form: "-w --systohc", default: false}, set: {form: "--set", default: false}, date: {form: "--date DATE"}, utc: {form: "-u --utc", default: false}, verbose: {form: "-v --verbose", default: false},
    help: {form: "-h --help", default: false, stop: true}, version: {form: "-V --version", default: false, stop: true}, operands: {form: "...ARG"},
  })?
  if opts.help { gnu.help("Usage: hwclock [-r|-s|-w|--set --date DATE] [-u] [-v]\nRead or set the RTC in UTC; drift adjustment is unavailable."); return }
  if opts.version { gnu.version("hwclock"); return }
  if ! opts.operands.is_empty() { gnu.extra_operand(opts.operands[0]) }
  let modes = (if opts.show { 1 } else { 0 }) + (if opts.hctosys { 1 } else { 0 }) + (if opts.systohc { 1 } else { 0 }) + (if opts.set { 1 } else { 0 })
  if modes > 1 { gnu.usage_error("clock operations are mutually exclusive") }
  if opts.set != (opts.date != null) { gnu.usage_error("--set requires --date and --date requires --set") }
  if opts.systohc {
    let epoch = time.now()
    if opts.verbose { gnu.write_text(f"Setting hardware clock to {epoch} milliseconds since epoch (UTC)\n") }
    linux.set_hwclock(epoch)
  } else if opts.set {
    let epoch = date_parse.parse(opts.date ?? "", utc: opts.utc)? / 1000000
    if opts.verbose { gnu.write_text(f"Setting hardware clock to {epoch} milliseconds since epoch (UTC)\n") }
    linux.set_hwclock(epoch)
  } else {
    if opts.verbose { gnu.write_text("Reading hardware clock in UTC\n") }
    let epoch = linux.hwclock()?
    if opts.hctosys { if opts.verbose { gnu.write_text("Setting system clock from hardware clock (UTC)\n") }; linux.set_system_clock(epoch) } else { gnu.write_text(time.format(control.epoch_nanoseconds(epoch)?, "%Y-%m-%d %H:%M:%S.%6N%:z", utc: true)? + "\n") }
  }
}
