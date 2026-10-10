#!/bin/xsh
use lib.gnu
use lib.smart
use lib.smart_ata as ata
use lib.smart_nvme as nvme
use lib.smart_scsi as scsi
use lib.smart_json as j

# smartctl over the linux storage transport: ATA through SAT pass-through,
# NVMe through the admin passthrough, and SCSI disks through SG_IO. Commands
# the transport does not expose (READ LOG EXT, CHECK POWER MODE, SET FEATURES,
# SMART autosave and automatic offline) are rejected by name rather than
# approximated.

const BANNER_RELEASE = "7.5 2025-04-30 r5714"
const COPYRIGHT = "Copyright (C) 2002-25, Bruce Allen, Christian Franke, www.smartmontools.org\n"
const USAGE_TRAILER = "\nUse smartctl -h to get a usage summary\n\n"

# Exit status bits smartctl documents.
const FAIL_COMMAND_LINE = 1
const FAIL_DEVICE = 2
const FAIL_SMART = 4
const FAIL_DISK = 8
const FAIL_PREFAIL = 16
const FAIL_PAST = 32
const FAIL_ERROR_LOG = 64
const FAIL_SELF_TEST_LOG = 128

const DEVICE_TYPES = "ata, scsi[+TYPE], nvme[,NSID], sat[,auto][,N][+TYPE], usbasm1352r,N, usbcypress[,X], usbjmicron[,p][,x][,N], usbprolific, usbsunplus[/sat], sntasmedia[/sat], sntjmicron[,NSID][/sat], sntrealtek[/sat], jmb39x[-q[2]],N[,sLBA][,force][+TYPE], jms56x,N[,sLBA][,force][+TYPE], areca,N/E, 3ware,N, hpt,L/M/N, megaraid,N, aacraid,H,L,ID, sssraid,E,S, cciss,N, auto, test"
const LOG_TYPES = "error, selftest, selective, directory[,g|s], xerror[,N][,error], xselftest[,N][,selftest], background, sasphy[,reset], sataphy[,reset], scttemp[sts,hist], scttempint,N[,p], scterc[,N,M][,p|reset], devstat[,N], defects[,N], ssd, gplog,N[,RANGE], smartlog,N[,RANGE], nvmelog,N,SIZE, tapedevstat, zdevstat, envrep, farm"
const TEST_TYPES = "offline, short, long, conveyance, force, vendor,N, select,M-N, pending,N, afterselect,[on|off]"
const SMART_VALUES = "on, off, aam,[N|off], apm,[N|off], dsn,[on|off], lookahead,[on|off], security-freeze, standby,[N|off|now], wcache,[on|off], rcache,[on|off], wcreorder,[on|off[,p]], wcache-sct,[ata|on|off[,p]]"

# Long option names with the short letter (or long-only key) they stand for and
# whether a value is absent (0), required (1) or optional (2).
const LONG_OPTIONS = [
  "help:h:0", "usage:h:0", "version:V:0", "copyright:V:0", "license:V:0", "info:i:0",
  "identify:identify:2", "get:g:1", "all:a:0", "xall:x:0", "scan:scan:0", "scan-open:scan-open:0",
  "json:j:2", "quietmode:q:1", "device:d:1", "tolerance:T:1", "badsum:b:1", "report:r:1",
  "nocheck:n:1", "smart:s:1", "offlineauto:o:1", "saveauto:S:1", "set:set:1", "health:H:0",
  "capabilities:c:0", "attributes:A:0", "format:f:1", "log:l:1", "vendorattribute:v:1",
  "firmwarebug:F:1", "presets:P:1", "drivedb:B:1", "test:t:1", "captive:C:0", "abort:X:0",
]

const SHORT_FLAGS = "h?ViaxHcACXj"
const SHORT_VALUES = "dlstnqTbrfvFPBoSg"

type Options = {
  json: Bool,
  compact: Bool,
  info: Bool,
  health: Bool,
  capabilities: Bool,
  attributes: Bool,
  all: Bool,
  xall: Bool,
  scan: Bool,
  scan_open: Bool,
  abort: Bool,
  device_type: Str,
  type_count: Int,
  logs: List[Str],
  test: Str?,
  smart_switch: Str?,
  quiet: Str,
  brief: Bool,
  badsum: Str,
  device: Str?,
}

# A command-line problem. `native` failures use smartctl's own wording and go
# to standard output after the banner; the others describe a feature this
# implementation lacks and go to standard error.
type Failure = {lines: List[Str], native: Bool}

# What the parser concluded: updated options, a failure, or an early exit.
type Applied = {options: Options, failure: Failure?, early: Str?}

type Clock = {epoch: Int, text: Str}

# One device's output: text and JSON members, the messages JSON reports, and
# the exit status bits it contributed.
type Session = {text: Str, members: List[j.Member], messages: List[Str], notes: List[Str], status: Int}

const DEFAULT_OPTIONS: Options = {
  json: false,
  compact: false,
  info: false,
  health: false,
  capabilities: false,
  attributes: false,
  all: false,
  xall: false,
  scan: false,
  scan_open: false,
  abort: false,
  device_type: "auto",
  type_count: 0,
  logs: [],
  test: null,
  smart_switch: null,
  quiet: "",
  brief: false,
  badsum: "warn",
  device: null,
}

pure invalid(option: Str, value: Str, valid: Str) -> Failure {
  {lines: [f"=======> INVALID ARGUMENT TO {option}: {value}", f"=======> VALID ARGUMENTS ARE: {valid} <======="], native: true}
}

pure unsupported(what: Str, why: Str) -> Failure {
  {lines: [f"{what} is not supported: {why}"], native: false}
}

pure rejected(options: Options, failure: Failure) -> Applied {
  {options: options, failure: failure, early: null}
}

pure accepted(options: Options) -> Applied {
  {options: options, failure: null, early: null}
}

pure head_word(value: Str) -> Str {
  value.split(",")[0]
}

# Applies one parsed option, validating its value the way smartctl does at
# parse time.
pure apply(options: Options, key: Str, value: Str) -> Applied {
  match key {
    "h" => {options: options, failure: null, early: "help"}
    "?" => {options: options, failure: null, early: "help"}
    "V" => {options: options, failure: null, early: "version"}
    "i" => accepted({...options, info: true})
    "a" => accepted({...options, all: true})
    "x" => accepted({...options, xall: true})
    "H" => accepted({...options, health: true})
    "c" => accepted({...options, capabilities: true})
    "A" => accepted({...options, attributes: true})
    "X" => {
      if options.abort or options.test != null { return rejected(options, single_test_failure()) }
      accepted({...options, abort: true})
    }
    "scan" => accepted({...options, scan: true})
    "scan-open" => accepted({...options, scan_open: true})
    "j" => apply_json(options, value)
    "d" => accepted({...options, device_type: value, type_count: options.type_count + 1})
    "l" => apply_log(options, value)
    "t" => apply_test(options, value)
    "s" => apply_switch(options, value)
    "n" => apply_nocheck(options, value)
    "q" => {
      if value in ["errorsonly", "silent", "noserial"] { return accepted({...options, quiet: value}) }
      rejected(options, invalid("-q", value, "errorsonly, silent, noserial"))
    }
    "T" => {
      if value == "normal" { return accepted(options) }
      if value in ["conservative", "permissive", "verypermissive"] {
        return rejected(options, unsupported(f"-T {value}", "only the normal tolerance level exists, because a failed mandatory command always ends the run"))
      }
      rejected(options, invalid("-T", value, "normal, conservative, permissive, verypermissive"))
    }
    "b" => {
      if value in ["warn", "exit", "ignore"] { return accepted({...options, badsum: value}) }
      rejected(options, invalid("-b", value, "warn, exit, ignore"))
    }
    "f" => {
      if value == "old" { return accepted({...options, brief: false}) }
      if value == "brief" { return accepted({...options, brief: true}) }
      if head_word(value) == "hex" { return rejected(options, unsupported(f"-f {value}", "hexadecimal attribute display is not implemented")) }
      rejected(options, invalid("-f", value, "old, brief, hex[,id|val]"))
    }
    "P" => {
      # Without a drive database no preset is ever applied, so `use` and
      # `ignore` behave alike.
      if value in ["use", "ignore"] { return accepted(options) }
      if value in ["show", "showall"] { return rejected(options, unsupported(f"-P {value}", "there is no drive database to show")) }
      rejected(options, invalid("-P", value, "use, ignore, show, showall"))
    }
    "F" => {
      if value == "none" { return accepted(options) }
      if value in ["nologdir", "samsung", "samsung2", "samsung3", "xerrorlba", "swapid"] {
        return rejected(options, unsupported(f"-F {value}", "firmware bug workarounds are not implemented"))
      }
      rejected(options, invalid("-F", value, "none, nologdir, samsung, samsung2, samsung3, xerrorlba, swapid"))
    }
    "r" => rejected(options, unsupported("-r", "transaction reports are not implemented"))
    "v" => rejected(options, unsupported("-v", "vendor attribute display overrides are not implemented"))
    "B" => rejected(options, unsupported("-B", "drive database files are not implemented"))
    "C" => rejected(options, unsupported("-C", "captive self-tests need a captive-mode command the transport does not offer"))
    "o" => rejected(options, unsupported("-o", "SMART automatic offline testing needs a command the transport does not expose"))
    "S" => rejected(options, unsupported("-S", "SMART attribute autosave needs a command the transport does not expose"))
    "g" => rejected(options, unsupported("-g", "device settings are read with commands the transport does not expose"))
    "set" => rejected(options, unsupported("--set", "device settings are changed with commands the transport does not expose"))
    "identify" => rejected(options, unsupported("--identify", "IDENTIFY DEVICE word dumps are not implemented"))
    _ => rejected(options, {lines: [f"=======> UNRECOGNIZED OPTION: {key}"], native: true})
  }
}

pure apply_json(options: Options, value: Str) -> Applied {
  if value == "" { return accepted({...options, json: true}) }
  var compact = false
  for letter in value.split("") {
    if letter == "c" {
      compact = true
    } else if letter in ["g", "i", "o", "s", "u", "v", "y"] {
      return rejected(options, unsupported(f"--json={value}", f"the '{letter}' format flag is not implemented"))
    } else {
      return rejected(options, invalid("--json", value, "cgiosuvy"))
    }
  }
  accepted({...options, json: true, compact: compact})
}

pure apply_log(options: Options, value: Str) -> Applied {
  let name = head_word(value)
  let known = ["error", "selftest", "selective", "directory", "xerror", "xselftest", "background", "sasphy", "sataphy", "scttemp", "scttempint", "scterc", "devstat", "defects", "ssd", "gplog", "smartlog", "nvmelog", "tapedevstat", "zdevstat", "envrep", "farm"]
  guard name in known else { return rejected(options, invalid("-l", value, LOG_TYPES)) }
  if name in ["error", "selftest", "selective"] and value == name {
    return accepted({...options, logs: options.logs + [name]})
  }
  if name == "directory" and value in ["directory", "directory,s"] {
    return accepted({...options, logs: options.logs + ["directory"]})
  }
  # The entry count (`xerror,N`) cannot be honored over SMART READ LOG, so
  # only the forms that name the summary log are accepted.
  if value in ["xerror", "xerror,error", "xselftest", "xselftest,selftest"] {
    return accepted({...options, logs: options.logs + [name]})
  }
  rejected(options, unsupported(f"-l {value}", "the log needs commands or a transport this implementation does not provide"))
}

pure single_test_failure() -> Failure {
  {lines: ["ERROR: smartctl can only run a single test type (or abort) at a time."], native: true}
}

pure apply_test(options: Options, value: Str) -> Applied {
  if options.test != null or options.abort { return rejected(options, single_test_failure()) }
  if value in ["offline", "short", "long", "conveyance"] { return accepted({...options, test: value}) }
  let name = head_word(value)
  if name in ["force", "vendor", "select", "pending", "afterselect"] {
    return rejected(options, unsupported(f"-t {value}", "only offline, short, long and conveyance self-tests can be started"))
  }
  rejected(options, invalid("-t", value, TEST_TYPES))
}

pure apply_switch(options: Options, value: Str) -> Applied {
  if value in ["on", "off"] { return accepted({...options, smart_switch: value}) }
  let name = head_word(value)
  if name in ["aam", "apm", "dsn", "lookahead", "security-freeze", "standby", "wcache", "rcache", "wcreorder", "wcache-sct"] {
    return rejected(options, unsupported(f"-s {value}", "device settings are changed with commands the transport does not expose"))
  }
  rejected(options, invalid("-s", value, SMART_VALUES))
}

pure apply_nocheck(options: Options, value: Str) -> Applied {
  if value == "never" { return accepted(options) }
  let name = head_word(value)
  if name in ["sleep", "standby", "idle"] {
    return rejected(options, unsupported(f"-n {value}", "the power mode is read with CHECK POWER MODE, which the transport does not expose"))
  }
  rejected(options, invalid("-n", value, "never, sleep[,STATUS[,STATUS2]], standby[,STATUS[,STATUS2]], idle[,STATUS[,STATUS2]]"))
}

# Resolves a long option name, accepting any unambiguous prefix as getopt_long
# does. Returns "key:arity" or null.
pure resolve_long(name: Str) -> Str? {
  var matches: List[Str] = []
  for entry in LONG_OPTIONS {
    let parts = entry.split(":")
    if parts[0] == name { return f"{parts[1]}:{parts[2]}" }
    if parts[0].starts_with(name) and f"{parts[1]}:{parts[2]}" not in matches { matches += [f"{parts[1]}:{parts[2]}"] }
  }
  if matches.len() == 1 { return matches[0] }
  null
}

type Parsed = {options: Options, operands: List[Str], failure: Failure?, early: Str?}

pure parse(argv: List[Str]) -> Parsed {
  var options = DEFAULT_OPTIONS
  var operands: List[Str] = []
  var index = 0
  var ended = false
  while index < argv.len() {
    let word = argv[index]
    index += 1
    if ended or !word.starts_with("-") or word == "-" {
      operands += [word]
      continue
    }
    if word == "--" {
      ended = true
      continue
    }
    if word.starts_with("--") {
      let body = word.byte_slice(2)
      let pieces = body.split("=", maxsplit: 1)
      guard let found = resolve_long(pieces[0]) else {
        return {options: options, operands: operands, failure: {lines: [f"=======> UNRECOGNIZED OPTION: {pieces[0]}"], native: true}, early: null}
      }
      let parts = found.split(":")
      var value = ""
      if parts[1] == "0" {
        if pieces.len() > 1 {
          return {options: options, operands: operands, failure: {lines: [f"=======> UNRECOGNIZED OPTION: {pieces[0]}"], native: true}, early: null}
        }
      } else if pieces.len() > 1 {
        value = pieces[1]
      } else if parts[1] == "1" {
        guard index < argv.len() else {
          return {options: options, operands: operands, failure: {lines: [f"=======> ARGUMENT REQUIRED FOR OPTION: {pieces[0]}"], native: true}, early: null}
        }
        value = argv[index]
        index += 1
      }
      let applied = apply(options, parts[0], value)
      if applied.failure != null or applied.early != null {
        return {options: applied.options, operands: operands, failure: applied.failure, early: applied.early}
      }
      options = applied.options
      continue
    }
    # A cluster of short options; a value option takes the rest of the word or
    # the next argument.
    let letters = word.byte_slice(1)
    var position = 0
    while position < letters.byte_len() {
      let letter = letters.byte_slice(position, 1)
      position += 1
      var value = ""
      if letter in SHORT_VALUES.split("") {
        if position < letters.byte_len() {
          value = letters.byte_slice(position)
          position = letters.byte_len()
        } else {
          guard index < argv.len() else {
            return {options: options, operands: operands, failure: {lines: [f"=======> ARGUMENT REQUIRED FOR OPTION: {letter}"], native: true}, early: null}
          }
          value = argv[index]
          index += 1
        }
      } else if letter not in SHORT_FLAGS.split("") {
        return {options: options, operands: operands, failure: {lines: [f"=======> UNRECOGNIZED OPTION: {letter}"], native: true}, early: null}
      }
      let applied = apply(options, letter, value)
      if applied.failure != null or applied.early != null {
        return {options: applied.options, operands: operands, failure: applied.failure, early: applied.early}
      }
      options = applied.options
    }
  }
  {options: options, operands: operands, failure: null, early: null}
}

pure platform(machine: Str, release: Str) -> Str {
  f"{machine}-linux-{release}"
}

pure banner(machine: Str, release: Str) -> Str {
  f"smartctl {BANNER_RELEASE} [{platform(machine, release)}] (XSH core)\n{COPYRIGHT}\n"
}

const HELP_TEXT = """Usage: smartctl [options] device

============================================ SHOW INFORMATION OPTIONS =====

  -h, --help, --usage
         Display this help and exit

  -V, --version, --copyright, --license
         Print license, copyright, and version information and exit

  -i, --info
         Show identity information for device

  -a, --all
         Show all SMART information for device

  -x, --xall
         Show all information for device

  --scan
         Scan for devices

  --scan-open
         Scan for devices and try to open each device

================================== SMARTCTL RUN-TIME BEHAVIOR OPTIONS =====

  -j, --json[=c]
         Print output in JSON format (c: compact)

  -q TYPE, --quietmode=TYPE
         Set smartctl quiet mode to one of: errorsonly, silent, noserial

  -d TYPE, --device=TYPE
         Specify device type to one of: ata, scsi, nvme[,0xNSID], sat[,auto][,N], auto
         (ata and sat both reach ATA disks through SAT pass-through)

  -T TYPE, --tolerance=TYPE
         Tolerance: normal

  -b TYPE, --badsum=TYPE
         Set action on bad checksum to one of: warn, exit, ignore

  -n MODE, --nocheck=MODE
         No check if: never

============================== DEVICE FEATURE ENABLE/DISABLE COMMANDS =====

  -s VALUE, --smart=VALUE
        Enable/disable SMART on device (on/off)

======================================= READ AND DISPLAY DATA OPTIONS =====

  -H, --health
        Show device SMART health status

  -c, --capabilities
        Show device SMART capabilities

  -A, --attributes
        Show device SMART vendor-specific Attributes and values

  -f FORMAT, --format=FORMAT
        Set output format for attributes: old, brief

  -l TYPE, --log=TYPE
        Show device log. TYPE: error, selftest, selective, directory,
        xerror, xselftest

============================================ DEVICE SELF-TEST OPTIONS =====

  -t TEST, --test=TEST
        Run test. TEST: offline, short, long, conveyance

  -X, --abort
        Abort any non-captive test on device

=================================================== SMARTCTL EXAMPLES =====

  smartctl --all /dev/sda                    (Prints all SMART information)

  smartctl --smart=on /dev/sda               (Enables SMART on first disk)

  smartctl --test=long /dev/sda          (Executes extended disk self-test)
"""

const VERSION_TEXT = """smartctl comes with ABSOLUTELY NO WARRANTY. This is free
software, and you are welcome to redistribute it under
the terms of the GNU General Public License; either
version 2, or (at your option) any later version.
See https://www.gnu.org for further details.

Output layouts follow smartmontools release 7.5.
"""

proc current_time() [time, error] -> Clock {
  let milliseconds = time.now()
  let text = time.format(milliseconds * 1000000, "%a %b %e %H:%M:%S %Y %Z")?
  {epoch: milliseconds / 1000, text: text}
}

# The JSON document: the fixed header, the smartctl block with its messages and
# exit status, then the members of the report.
proc json_document(options: Options, argv: List[Str], machine: Str, release: Str, messages: List[Str], notes: List[Str], status: Int, members: List[j.Member]) -> Str {
  var block: List[j.Member] = [
    j.m("version", [7, 5]),
    j.m("pre_release", false),
    j.m("svn_revision", "5714"),
    j.m("platform_info", platform(machine, release)),
    j.m("build_info", "(XSH core)"),
    j.m("argv", ["smartctl"] + argv),
  ]
  if !messages.is_empty() or !notes.is_empty() {
    var entries: List[j.Object] = []
    for message in messages { entries += [j.object([j.m("string", message), j.m("severity", "error")])] }
    for note in notes { entries += [j.object([j.m("string", note), j.m("severity", "information")])] }
    block += [j.m("messages", entries)]
  }
  block += [j.m("exit_status", status)]
  let document = j.object([j.m("json_format_version", [1, 0]), j.m("smartctl", j.object(block))] + members)
  j.render(document, compact: options.compact) + "\n"
}

proc finish(options: Options, argv: List[Str], machine: Str, release: Str, session: Session) {
  var status = session.status
  if options.json {
    gnu.write_text(json_document(options, argv, machine, release, session.messages, session.notes, status, session.members))
  } else if options.quiet != "silent" {
    gnu.write_text(session.text)
  }
  exit status
}

pure fail_text(lines: List[Str]) -> Str {
  lines.join("\n") + "\n"
}

# A device read reduced to its data or the kernel's reason for failing.
type Fetched = {ok: Bool, data: Bytes, message: Str}

pure fetched(data: Bytes) -> Fetched {
  {ok: true, data: data, message: ""}
}

pure unfetched(failure: Error) -> Fetched {
  {ok: false, data: b"", message: gnu.strerror(failure)}
}

# One ATA page: identify, SMART data, thresholds, or a SMART log.
proc ata_page(fd: Int, size: Int, command: Str, address: Int) [process] -> Fetched {
  let reply = match command {
    "identify" => linux.ata_identify(fd, size)
    "values" => linux.ata_smart_read_data(fd, size)
    "thresholds" => linux.ata_smart_read_thresholds(fd, size)
    _ => linux.ata_smart_read_log(fd, address, 1, size)
  }
  match reply {
    Ok(result) => fetched(result.data)
    Err(failure) => unfetched(failure)
  }
}

type Verdict = {ok: Bool, passed: Bool, message: Str}

proc ata_verdict(fd: Int, size: Int) [process] -> Verdict {
  match linux.ata_smart_status(fd, size) {
    Ok(result) => {ok: true, passed: result.passed, message: ""}
    Err(failure) => {ok: false, passed: false, message: gnu.strerror(failure)}
  }
}

# A mutating ATA command: SMART enable, disable, or a self-test subcommand.
proc ata_command(fd: Int, size: Int, command: Str) [process] -> Fetched {
  let reply = match command {
    "enable" => linux.ata_smart_enable(fd, size)
    "disable" => linux.ata_smart_disable(fd, size)
    _ => linux.ata_smart_start_self_test(fd, command, size)
  }
  match reply {
    Ok(result) => fetched(result.data)
    Err(failure) => unfetched(failure)
  }
}

# NVMe admin reads.
proc nvme_page(fd: Int, command: Str, number: Int, length: Int) [process] -> Fetched {
  let reply = match command {
    "controller" => linux.nvme_identify_controller(fd)
    "namespace" => linux.nvme_identify_namespace(fd, number)
    _ => linux.nvme_log_page(fd, number, length)
  }
  match reply {
    Ok(result) => fetched(result.data)
    Err(failure) => unfetched(failure)
  }
}

# The namespace id of a namespace node, or null for a controller node.
proc nvme_namespace(fd: Int) [process] -> Int? {
  match linux.nvme_namespace_id(fd) {
    Ok(found) => found
    Err(_) => null
  }
}

proc nvme_self_test(fd: Int, code: Int) [process] -> Fetched {
  match linux.nvme_admin_command(fd, 20, 0, nsid: 4294967295, cdw10: code) {
    Ok(result) => fetched(result.data)
    Err(failure) => unfetched(failure)
  }
}

# The ATA control flow: identify, optional enable or disable, the read
# sections, then any self-test start.
proc ata_session(options: Options, fd: Int, size: Int, now: Clock) [process, time, error] -> Session {
  var members: List[j.Member] = []
  var messages: List[Str] = []
  var notes: List[Str] = []
  var status = 0
  let identity = ata_page(fd, size, "identify", 0)
  if !identity.ok {
    let line = f"Read Device Identity failed: {identity.message}"
    return {
      text: line + "\n\nIf this is a USB connected device, look at the various --device=TYPE variants\nA mandatory SMART command failed: exiting. To continue, add one or more '-T permissive' options.\n",
      members: [],
      messages: [line],
      notes: [],
      status: FAIL_DEVICE,
    }
  }
  let page = identity.data
  let w = smart.words(page)
  var text = ""
  var enabled = smart.smart_enabled(w)
  let supported = smart.smart_supported(w)
  let everything = options.all or options.xall
  let want_info = options.info or everything
  let want_health = options.health or everything
  let want_capabilities = options.capabilities or everything
  let want_attributes = options.attributes or everything
  var logs = options.logs
  if everything {
    for log in ["error", "selftest", "selective"] { if log not in logs { logs += [log] } }
  }
  if options.xall and "directory" not in logs { logs += ["directory"] }
  let brief = options.brief or options.xall
  if want_info {
    text += ata.info_text(page, now.text, options.quiet != "noserial")
    members += ata.info_json(page, options.quiet != "noserial")
  }
  if let switch = options.smart_switch {
    text += "=== START OF ENABLE/DISABLE COMMANDS SECTION ===\n"
    if supported != 1 {
      let line = "SMART Enable/Disable failed: the device does not report SMART support"
      text += line + "\n\n"
      messages += [line]
      status = status.bit_or(FAIL_SMART)
    } else {
      let outcome = ata_command(fd, size, if switch == "on" { "enable" } else { "disable" })
      if !outcome.ok {
        let line = f"SMART Enable/Disable failed: {outcome.message}"
        text += line + "\n\n"
        messages += [line]
        status = status.bit_or(FAIL_SMART)
      } else if switch == "on" {
        text += "SMART Enabled.\n\n"
        notes += ["SMART Enabled."]
        enabled = 1
      } else {
        text += "SMART Disabled. Use option -s with argument 'on' to enable it.\n\n"
        notes += ["SMART Disabled. Use option -s with argument 'on' to enable it."]
        enabled = 0
      }
    }
  }
  var body = ""
  var read_failed = false
  let needs_read = want_health or want_capabilities or want_attributes or !logs.is_empty() or options.test != null
  if needs_read and supported == 0 {
    body += "SMART support is: Unavailable - device lacks SMART capability.\n\n"
    messages += ["SMART support is: Unavailable - device lacks SMART capability."]
    status = status.bit_or(FAIL_SMART)
    read_failed = true
  } else if needs_read and enabled == 0 {
    body += "SMART Disabled. Use option -s with argument 'on' to enable it.\n\n"
    messages += ["SMART Disabled. Use option -s with argument 'on' to enable it."]
    status = status.bit_or(FAIL_SMART)
    read_failed = true
  }
  # SMART data (and thresholds) serve -c and -A, name the failed or marginal
  # attributes beside the health verdict, and give the self-test time.
  var parsed: smart.SmartValues? = null
  let needs_values = !read_failed and (want_capabilities or want_attributes or options.test != null or "selective" in logs)
  if !read_failed and (needs_values or want_health) {
    let values = ata_page(fd, size, "values", 0)
    if !values.ok {
      if needs_values {
        let line = f"Read SMART Data failed: {values.message}"
        body += line + "\n\n"
        messages += [line]
        status = status.bit_or(FAIL_SMART)
      }
    } else {
      var usable = true
      if !smart.checksum_ok(values.data) and options.badsum != "ignore" {
        if needs_values {
          let line = "Warning! SMART Attribute Data Structure error: invalid SMART checksum."
          body += line + "\n"
          messages += [line]
          status = status.bit_or(FAIL_SMART)
          if options.badsum == "exit" {
            return {text: text + ata.READ_HEADER + body, members: members, messages: messages, notes: notes, status: status}
          }
        } else {
          usable = false
        }
      }
      var thresholds: Bytes? = null
      if usable and (want_attributes or want_health) {
        let table = ata_page(fd, size, "thresholds", 0)
        if table.ok {
          thresholds = table.data
        } else if want_attributes {
          let line = f"Read SMART Thresholds failed: {table.message}"
          body += line + "\n"
          messages += [line]
          status = status.bit_or(FAIL_SMART)
        }
      }
      if usable { parsed = smart.smart_values(values.data, thresholds) }
    }
  }
  if !read_failed and want_health {
    let verdict = ata_verdict(fd, size)
    if verdict.ok {
      if options.quiet != "errorsonly" or !verdict.passed { body += ata.health_text(verdict.passed, parsed) }
      members += [j.m("smart_status", j.object([j.m("passed", verdict.passed)]))]
      if !verdict.passed { status = status.bit_or(FAIL_DISK) }
    } else {
      let line = f"SMART Status command failed: {verdict.message}"
      body += line + "\n\n"
      messages += [line]
      status = status.bit_or(FAIL_SMART)
    }
  }
  if let values = parsed {
    if want_capabilities {
      if options.quiet != "errorsonly" { body += ata.capabilities_text(values, page) }
      members += ata.capabilities_json(values, page)
    }
    var failing = false
    for attribute in values.attributes {
      if attribute.state == "failed_now" and attribute.prefailure { status = status.bit_or(FAIL_PREFAIL); failing = true }
      if attribute.state == "failed_past" or (attribute.state == "failed_now" and !attribute.prefailure) { status = status.bit_or(FAIL_PAST); failing = true }
    }
    if want_attributes {
      if options.quiet != "errorsonly" or failing { body += ata.attributes_text(values, brief) }
      members += ata.attributes_json(values)
    }
  }
  if !read_failed {
    var summary_notice = false
    var seen: List[Int] = []
    for log in logs {
      var address = 0
      var label = "SMART Log Directory"
      if log in ["error", "xerror"] {
        address = 1
        label = "SMART Error Log"
      } else if log in ["selftest", "xselftest"] {
        address = 6
        label = "SMART Self-test Log"
      } else if log == "selective" {
        address = 9
        label = "SMART Selective Self-test Log"
      }
      if log in ["xerror", "xselftest"] { summary_notice = true }
      continue when address in seen
      seen += [address]
      let reading = ata_page(fd, size, "log", address)
      if !reading.ok {
        let line = f"Read {label} failed: {reading.message}"
        body += line + "\n\n"
        messages += [line]
        status = status.bit_or(FAIL_SMART)
        continue
      }
      let data = reading.data
      if address != 0 and !smart.checksum_ok(data) and options.badsum != "ignore" {
        let line = f"Warning! {label} error: invalid SMART checksum."
        body += line + "\n"
        messages += [line]
        status = status.bit_or(FAIL_SMART)
        if options.badsum == "exit" {
          return {text: text + ata.READ_HEADER + body, members: members, messages: messages, notes: notes, status: status}
        }
      }
      if address == 1 {
        let decoded = smart.error_log(data)
        if decoded.count > 0 { status = status.bit_or(FAIL_ERROR_LOG) }
        if options.quiet != "errorsonly" or decoded.count > 0 { body += ata.error_log_text(decoded) }
        members += ata.error_log_json(decoded)
      } else if address == 6 {
        let decoded = smart.self_test_log(data)
        let current_errors = decoded.error_count - decoded.outdated_count
        if current_errors > 0 { status = status.bit_or(FAIL_SELF_TEST_LOG) }
        if options.quiet != "errorsonly" or current_errors > 0 { body += ata.self_test_log_text(decoded) }
        members += ata.self_test_log_json(decoded)
      } else if address == 9 {
        let decoded = smart.selective_log(data)
        var exec = 0
        if let values = parsed { exec = values.self_test_status }
        if options.quiet != "errorsonly" { body += ata.selective_log_text(decoded, exec) }
        members += ata.selective_log_json(decoded, exec)
      } else {
        let decoded = smart.log_directory(data)
        if options.quiet != "errorsonly" { body += ata.directory_text(decoded) }
        members += ata.directory_json(decoded)
      }
    }
    if summary_notice {
      eprint "smartctl: -l xerror and -l xselftest show the summary logs: READ LOG EXT is not available through this transport"
    }
  }
  if body != "" { text += ata.READ_HEADER + body }
  if !read_failed and (options.test != null or options.abort) {
    var sub = "short"
    var title = "Short self-test"
    if let requested = options.test {
      sub = if requested == "long" { "extended" } else { requested }
      title = if requested == "offline" { "Immediate Offline" } else if requested == "short" { "Short self-test" } else if requested == "long" { "Extended self-test" } else { "Conveyance self-test" }
    }
    text += "=== START OF OFFLINE IMMEDIATE AND SELF-TEST SECTION ===\n"
    var action = f"Execute SMART {title} routine immediately in off-line mode"
    if options.abort {
      sub = "abort"
      action = "Abort SMART off-line mode self-test routine"
    }
    text += f"Sending command: \"{action}\".\n"
    notes += [f"Sending command: \"{action}\"."]
    let started = ata_command(fd, size, sub)
    if !started.ok {
      let line = f"Command \"{action}\" failed: {started.message}"
      text += line + "\n\n"
      messages += [line]
      status = status.bit_or(FAIL_SMART)
    } else if options.abort {
      text += "Self-testing aborted!\n\n"
      notes += ["Self-testing aborted!"]
    } else {
      text += f"Drive command \"{action}\" successful.\nTesting has begun.\n"
      notes += [f"Drive command \"{action}\" successful.", "Testing has begun."]
      if let values = parsed {
        var minutes = values.conveyance_minutes
        if sub == "short" { minutes = values.short_minutes }
        if sub == "extended" { minutes = values.extended_minutes }
        let seconds = if sub == "offline" { values.offline_seconds } else { minutes * 60 }
        let unit = if sub == "offline" { f"{seconds} seconds" } else if minutes == 1 { "1 minute" } else { f"{minutes} minutes" }
        text += f"Please wait {unit} for test to complete.\n"
        notes += [f"Please wait {unit} for test to complete."]
        let done = time.format((now.epoch + seconds) * 1000000000, "%a %b %e %H:%M:%S %Y %Z") ?? ""
        text += f"Test will complete after {done}\n"
        notes += [f"Test will complete after {done}"]
      }
      text += "Use smartctl -X to abort test.\n\n"
      notes += ["Use smartctl -X to abort test."]
    }
  }
  {text: text, members: members, messages: messages, notes: notes, status: status}
}

# NVMe sessions: identify controller, namespace, then logs.
proc nvme_session(options: Options, fd: Int, nsid_hint: Int?, now: Clock) [process, error] -> Session {
  var members: List[j.Member] = []
  var messages: List[Str] = []
  var notes: List[Str] = []
  var status = 0
  let identified = nvme_page(fd, "controller", 0, 4096)
  if !identified.ok {
    let line = f"Read NVMe Identify Controller failed: NVME_IOCTL_ADMIN_CMD: {identified.message}"
    return {text: line + "\n", members: [], messages: [line], notes: [], status: FAIL_DEVICE}
  }
  let controller = nvme.controller(identified.data)
  var nsid = 1
  if let hint = nsid_hint {
    nsid = hint
  } else if let found = nvme_namespace(fd) {
    nsid = found
  }
  var namespace: nvme.Namespace? = null
  let described = nvme_page(fd, "namespace", nsid, 4096)
  if described.ok { namespace = nvme.namespace(described.data) }
  let everything = options.all or options.xall
  let want_info = options.info or everything
  let want_health = options.health or everything
  let want_capabilities = options.capabilities or everything
  let want_attributes = options.attributes or everything
  var logs = options.logs
  if everything {
    for log in ["error", "selftest"] { if log not in logs { logs += [log] } }
  }
  var text = ""
  if want_info or want_capabilities {
    text += "=== START OF INFORMATION SECTION ===\n"
    if want_info {
      let block = nvme.info_text(controller, namespace, nsid, now.text, options.quiet != "noserial")
      text += block.byte_slice((block.find("\n") ?? 0) + 1)
      if !want_capabilities { text += "\n" }
      members += nvme.info_json(controller, namespace, nsid, options.quiet != "noserial")
    }
    if want_capabilities { text += nvme.capabilities_text(controller, namespace, nsid, identified.data) }
  }
  var body = ""
  var health: nvme.Health? = null
  if want_health or want_attributes {
    let reading = nvme_page(fd, "log", 2, 512)
    if reading.ok {
      health = nvme.health(reading.data)
    } else {
      let line = f"Read NVMe SMART/Health Information failed: {reading.message}"
      body += line + "\n\n"
      messages += [line]
      status = status.bit_or(FAIL_SMART)
    }
  }
  var error_entries = 0
  if let log = health {
    error_entries = log.error_entries
    if want_health {
      if log.critical_warning != 0 { status = status.bit_or(FAIL_DISK) }
      if options.quiet != "errorsonly" or log.critical_warning != 0 { body += nvme.health_text(log) }
      members += [j.m("smart_status", j.object([j.m("passed", log.critical_warning == 0), j.m("nvme", nvme.warning_json(log.critical_warning))]))]
    }
    if want_attributes {
      if options.quiet != "errorsonly" { body += nvme.health_log_text(log, 4294967295) }
      members += nvme.health_json(log)
    }
  }
  for log in logs {
    if log in ["error", "xerror"] {
      let capacity = smart.at(identified.data, 262) + 1
      let count = if capacity > 16 { 16 } else { capacity }
      let reading = nvme_page(fd, "log", 1, count * 64)
      if reading.ok {
        let records = nvme.error_records(reading.data)
        if error_entries > 0 { status = status.bit_or(FAIL_ERROR_LOG) }
        if options.quiet != "errorsonly" or error_entries > 0 { body += nvme.error_log_text(records, count, capacity, error_entries) }
        members += nvme.error_log_json(records, count, capacity, error_entries)
      } else {
        let line = f"Read Error Information Log failed: {reading.message}"
        body += line + "\n\n"
        messages += [line]
        status = status.bit_or(FAIL_SMART)
      }
    } else if log in ["selftest", "xselftest"] {
      # Without the Device Self-test command the log page does not exist.
      if !smart.bit_set(controller.oacs, 4) {
        if log in options.logs and !everything {
          let line = "Self-tests not supported"
          body += line + "\n\n"
          messages += [line]
        }
        continue
      }
      let reading = nvme_page(fd, "log", 6, 564)
      if reading.ok {
        let decoded = nvme.self_test_log(reading.data)
        var failed = false
        for entry in decoded.entries { if entry.result >= 5 and entry.result <= 7 { failed = true } }
        if failed { status = status.bit_or(FAIL_SELF_TEST_LOG) }
        if options.quiet != "errorsonly" or failed { body += nvme.self_test_log_text(decoded, 4294967295) }
        members += nvme.self_test_log_json(decoded)
      } else {
        let line = f"Read Self-test Log failed: {reading.message}"
        body += line + "\n\n"
        messages += [line]
        status = status.bit_or(FAIL_SMART)
      }
    }
  }
  if body != "" { text += "=== START OF SMART DATA SECTION ===\n" + body }
  if options.test != null or options.abort {
    text += "=== START OF OFFLINE IMMEDIATE AND SELF-TEST SECTION ===\n"
    # Device Self-test: 1 short, 2 extended, 0xf abort; the broadcast
    # namespace id asks the controller to test every namespace.
    let code = if options.abort { 15 } else if options.test == "long" { 2 } else { 1 }
    let started = nvme_self_test(fd, code)
    if !started.ok {
      let line = f"Self-test command failed: {started.message}"
      text += line + "\n\n"
      messages += [line]
      status = status.bit_or(FAIL_SMART)
    } else if options.abort {
      text += "Self-test aborted!\n\n"
      notes += ["Self-test aborted!"]
    } else {
      text += "Self-test has begun\nUse smartctl -X to abort test\n\n"
      notes += ["Self-test has begun", "Use smartctl -X to abort test"]
    }
  }
  {text: text, members: members, messages: messages, notes: notes, status: status}
}

# One SCSI read, answered as the data it returned or the failure text.
proc scsi_read(fd: Int, cdb: Bytes, length: Int) [process] -> Fetched {
  match linux.sg_io(fd, cdb, length) {
    Ok(reply) => {
      if reply.status != 0 { return {ok: false, data: b"", message: f"SCSI status 0x{smart.hex(reply.status, 2)}"} }
      fetched(reply.data)
    }
    Err(failure) => unfetched(failure)
  }
}

proc scsi_session(options: Options, fd: Int, now: Clock) [process, error] -> Session {
  var members: List[j.Member] = []
  var messages: List[Str] = []
  var notes: List[Str] = []
  var status = 0
  var text = ""
  let standard = scsi_read(fd, b"\x12\x00\x00\x00\x24\x00", 36)
  if !standard.ok {
    let first = f"Standard Inquiry (36 bytes) failed [{standard.message}]"
    return {
      text: f"{first}\nRetrying with a 64 byte Standard Inquiry\nStandard Inquiry (64 bytes) failed [{standard.message}]\nA mandatory SMART command failed: exiting. To continue, add one or more '-T permissive' options.\n",
      members: [],
      messages: [first],
      notes: [],
      status: FAIL_DEVICE,
    }
  }
  let id = scsi.inquiry(standard.data)
  var serial: Str? = null
  let unit = scsi_read(fd, b"\x12\x01\x80\x00\xff\x00", 255)
  if unit.ok { serial = scsi.serial_number(unit.data) }
  var capacity: scsi.Capacity? = null
  let short = scsi_read(fd, b"\x25\x00\x00\x00\x00\x00\x00\x00\x00\x00", 8)
  if short.ok {
    let last = scsi.capacity10(short.data)
    if last.last == 4294967295 {
      let long = scsi_read(fd, b"\x9e\x10\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x20\x00\x00", 32)
      if long.ok { capacity = scsi.capacity16(long.data) }
    } else {
      capacity = last
    }
  }
  let everything = options.all or options.xall
  let hidden_serial: Str? = if options.quiet == "noserial" { null } else { serial }
  if options.info or everything {
    text += scsi.info_text(id, hidden_serial, capacity, now.text)
    members += scsi.info_json(id, hidden_serial, capacity)
  }
  var body = ""
  let want_health = options.health or everything
  let want_attributes = options.attributes or everything
  if want_health {
    let log = scsi_read(fd, b"\x4d\x00\x6f\x00\x00\x00\x00\x00\xfc\x00", 252)
    if log.ok {
      let report = scsi.exceptions(log.data)
      if report.asc != 0 { status = status.bit_or(FAIL_DISK) }
      if options.quiet != "errorsonly" or report.asc != 0 { body += scsi.health_text(report) }
      members += scsi.health_json(report)
      if want_attributes { body += "\n" }
    } else {
      let line = f"Read SMART Health Status failed: {log.message}"
      body += line + "\n"
      messages += [line]
      status = status.bit_or(FAIL_SMART)
    }
  }
  if want_attributes {
    let page = scsi_read(fd, b"\x4d\x00\x4d\x00\x00\x00\x00\x00\xfc\x00", 252)
    if page.ok {
      let temperatures = scsi.temperatures(page.data)
      body += scsi.temperature_text(temperatures)
      members += scsi.temperature_json(temperatures)
    } else {
      let line = f"Read Temperature log page failed: {page.message}"
      body += line + "\n\n"
      messages += [line]
      status = status.bit_or(FAIL_SMART)
    }
  }
  if body != "" { text += scsi.READ_HEADER + body }
  {text: text, members: members, messages: messages, notes: notes, status: status}
}

# Candidate lines for --scan and --scan-open, in smartctl's scan format.
pure scan_line(node: Str, kind: Str, ata_behind: Bool) -> Str {
  if kind == "nvme" { return f"{node} -d nvme # {node}, NVMe device" }
  if ata_behind { return f"{node} -d sat # {node} [SAT], ATA device" }
  f"{node} -d scsi # {node}, SCSI device"
}

proc scan(options: Options) [fs, process, error] -> Session {
  let family = head_word(options.device_type.split("+")[0])
  var text = ""
  var devices: List[j.Object] = []
  var messages: List[Str] = []
  let found = smart.present_candidates(fp"/sys", fp"/dev")?
  for candidate in found {
    # Controllers carry the NVMe admin node; namespaces are reached through
    # them.
    continue when candidate.kind == "nvme_namespace"
    # `--scan -d TYPE` lists only devices of that family.
    continue when family == "nvme" and candidate.protocol != "nvme"
    continue when family in ["scsi", "ata", "sat"] and candidate.protocol == "nvme"
    let node = f"{candidate.path}"
    var ata_behind = false
    var open_error: Str? = null
    let kind = if candidate.protocol == "nvme" { "nvme" } else { "scsi" }
    if options.scan_open {
      match unix.open_fd(candidate.path, nonblock: true) {
        Ok(fd) => {
          if candidate.kind == "scsi_disk" {
            let reply = scsi_read(fd, b"\x12\x00\x00\x00\x24\x00", 36)
            if reply.ok { ata_behind = scsi.is_ata(scsi.inquiry(reply.data)) } else { open_error = f"INQUIRY: {reply.message}" }
          }
          unix.close_fd(fd)?
        }
        Err(failure) => { open_error = gnu.strerror(failure) }
      }
    }
    let transport = if kind == "nvme" { "nvme" } else if ata_behind { "sat" } else { "scsi" }
    let protocol = if kind == "nvme" { "NVMe" } else if ata_behind { "ATA" } else { "SCSI" }
    let info = if ata_behind { f"{node} [SAT]" } else { node }
    var entry: List[j.Member] = [j.m("name", node), j.m("info_name", info), j.m("type", transport), j.m("protocol", protocol)]
    if let reason = open_error {
      text += f"# {node} -d {transport} # {info}, {protocol} device open failed: {reason}\n"
      entry += [j.m("open_error", reason)]
    } else {
      text += scan_line(node, kind, ata_behind) + "\n"
    }
    devices += [j.object(entry)]
  }
  {text: text, members: [j.m("devices", devices)], messages: messages, notes: [], status: 0}
}

proc main(...argv: List[Str]) [fs, process, time, env, error, io] {
  let uname = system.uname()?
  let parsed = parse(argv)
  let intro = banner(uname.machine, uname.release)
  var options = parsed.options
  # JSON mode is chosen before any complaint so the complaint is structured.
  if let failure = parsed.failure {
    if options.json {
      gnu.write_text(json_document(options, argv, uname.machine, uname.release, failure.lines, [], FAIL_COMMAND_LINE, []))
    } else if failure.native {
      gnu.write_text(intro + fail_text(failure.lines) + USAGE_TRAILER)
    } else {
      for line in failure.lines { gnu.error(line) }
    }
    exit FAIL_COMMAND_LINE
  }
  if let early = parsed.early {
    if early == "help" {
      gnu.write_text(intro + HELP_TEXT)
    } else {
      gnu.write_text(intro + VERSION_TEXT)
    }
    return
  }
  let clock_now = current_time()
  let quiet_intro = options.json or options.quiet != ""
  if options.scan or options.scan_open {
    # smartctl scans regardless of any device operand; only the type filter
    # needs to name a family the scan can honor.
    let family = head_word(options.device_type.split("+")[0])
    if family not in ["auto", "ata", "sat", "scsi", "nvme"] {
      let lines = [f"Unknown device type '{options.device_type}'", f"=======> VALID ARGUMENTS ARE: {DEVICE_TYPES} <======="]
      if options.json {
        gnu.write_text(json_document(options, argv, uname.machine, uname.release, lines, [], FAIL_COMMAND_LINE, []))
      } else {
        gnu.write_text(intro + fail_text(lines) + USAGE_TRAILER)
      }
      exit FAIL_COMMAND_LINE
    }
    finish(options, argv, uname.machine, uname.release, scan(options))
  }
  if options.type_count > 1 {
    let message = "ERROR: multiple -d TYPE options are only allowed with --scan"
    if options.json {
      gnu.write_text(json_document(options, argv, uname.machine, uname.release, [message], [], FAIL_COMMAND_LINE, []))
    } else {
      gnu.write_text(intro + message + "\n" + USAGE_TRAILER)
    }
    exit FAIL_COMMAND_LINE
  }
  if parsed.operands.is_empty() {
    let message = "ERROR: smartctl requires a device name as the final command-line argument."
    if options.json {
      gnu.write_text(json_document(options, argv, uname.machine, uname.release, [message], [], FAIL_COMMAND_LINE, []))
    } else {
      gnu.write_text(intro + message + "\n\n" + USAGE_TRAILER)
    }
    exit FAIL_COMMAND_LINE
  }
  if parsed.operands.len() > 1 {
    var lines = ["ERROR: smartctl takes ONE device name as the final command-line argument.", f"You have provided {parsed.operands.len()} device names:"]
    for operand in parsed.operands { lines += [operand] }
    if options.json {
      gnu.write_text(json_document(options, argv, uname.machine, uname.release, lines, [], FAIL_COMMAND_LINE, []))
    } else {
      gnu.write_text(intro + fail_text(lines) + USAGE_TRAILER)
    }
    exit FAIL_COMMAND_LINE
  }
  let name = parsed.operands[0]
  options = {...options, device: name}
  let leader = if quiet_intro { "" } else { intro }

  # Settle the device type before opening anything.
  let requested = options.device_type
  let family = head_word(requested.split("+")[0])
  var kind = ""
  var cdb_size = 16
  var nsid_hint: Int? = null
  var sat_probe = false
  var protocol_name = ""
  var info_name = name
  var fault: Failure? = null
  if family == "auto" {
    let base = fp"{name}".basename()
    if base.starts_with("nvme") {
      kind = "nvme"
    } else if rx"^(sd[a-z]+|hd[a-z]+|sg[0-9]+|bsg|nst[0-9]+|st[0-9]+)$".matches(base) {
      kind = "auto"
    } else {
      fault = {lines: [f"{name}: Unable to detect device type", "Please specify device type with the -d option."], native: true}
    }
  } else if family == "ata" {
    kind = "ata"
  } else if family == "scsi" {
    kind = "scsi"
    if "+" in requested { fault = unsupported(f"-d {requested}", "tunnelled SCSI device types are not implemented") }
  } else if family == "sat" {
    kind = "sat"
    let parts = requested.split(",")
    for part in parts[1..] {
      if part == "12" {
        cdb_size = 12
      } else if part == "16" {
        cdb_size = 16
      } else if part == "0" {
        cdb_size = 16
      } else if part == "auto" {
        sat_probe = true
      } else {
        fault = {lines: [f"{name}: Option '-d sat[,auto][,N]' requires N to be 0, 12 or 16", f"=======> VALID ARGUMENTS ARE: {DEVICE_TYPES} <======="], native: true}
      }
    }
  } else if family == "nvme" {
    kind = "nvme"
    let parts = requested.split(",")
    if parts.len() > 1 {
      # smartctl takes the namespace id as 0x-prefixed hexadecimal.
      let digits = if parts[1].starts_with("0x") { parts[1].byte_slice(2) } else { "" }
      var value = 0
      var valid = digits != ""
      for letter in digits.lower().split("") {
        let digit = "0123456789abcdef".find(letter)
        if digit == null { valid = false } else { value = value * 16 + (digit ?? 0) }
      }
      if !valid or value < 1 or value > 4294967295 {
        fault = {lines: [f"{name}: Invalid NVMe namespace id in '{requested}'", f"=======> VALID ARGUMENTS ARE: {DEVICE_TYPES} <======="], native: true}
      } else {
        nsid_hint = value
      }
    }
  } else if family in ["usbasm1352r", "usbcypress", "usbjmicron", "usbprolific", "usbsunplus", "sntasmedia", "sntjmicron", "sntrealtek", "jmb39x", "jmb39x-q", "jmb39x-q2", "jms56x", "areca", "3ware", "hpt", "megaraid", "aacraid", "sssraid", "cciss", "test"] {
    fault = unsupported(f"-d {requested}", "only ata, scsi, sat and nvme transports are implemented")
  } else {
    fault = {lines: [f"{name}: Unknown device type '{requested}'", f"=======> VALID ARGUMENTS ARE: {DEVICE_TYPES} <======="], native: true}
  }
  if let problem = fault {
    var failure_status = FAIL_COMMAND_LINE
    if options.json {
      finish(options, argv, uname.machine, uname.release, {text: "", members: [], messages: problem.lines, notes: [], status: failure_status})
    }
    if problem.native {
      gnu.write_text(leader + fail_text(problem.lines) + USAGE_TRAILER)
    } else {
      for line in problem.lines { gnu.error(line) }
    }
    exit failure_status
  }

  var descriptor = -1
  match unix.open_fd(fp"{name}", nonblock: true) {
    Ok(opened) => { descriptor = opened }
    Err(failure) => {
      let line = f"Smartctl open device: {name} failed: {gnu.strerror(failure)}"
      finish(options, argv, uname.machine, uname.release, {text: leader + line + "\n", members: device_members(clock_now, name, name, "", ""), messages: [line], notes: [], status: FAIL_DEVICE})
    }
  }
  let fd = descriptor
  defer unix.close_fd(fd)

  # An unknown family is told apart by the standard INQUIRY: an ATA vendor
  # identifier means a SAT translator in front of an ATA disk.
  if kind == "auto" {
    let probe = scsi_read(fd, b"\x12\x00\x00\x00\x24\x00", 36)
    if probe.ok {
      kind = if scsi.is_ata(scsi.inquiry(probe.data)) { "sat" } else { "scsi" }
    } else {
      let message = f"{name}: Unable to detect device type"
      if options.json {
        finish(options, argv, uname.machine, uname.release, {text: "", members: [], messages: [message], notes: [], status: FAIL_COMMAND_LINE})
      }
      gnu.write_text(leader + message + "\nPlease specify device type with the -d option.\n" + USAGE_TRAILER)
      exit FAIL_COMMAND_LINE
    }
  }
  if kind == "sat" and sat_probe {
    # `sat,auto` asks the device with INQUIRY before trusting it to be SAT.
    let probe = scsi_read(fd, b"\x12\x00\x00\x00\x24\x00", 36)
    if !probe.ok {
      let line = f"Smartctl open device: {name} [SCSI/SAT] failed: INQUIRY [SAT]: {probe.message}"
      finish(options, argv, uname.machine, uname.release, {text: leader + line + "\n", members: device_members(clock_now, name, name, "", ""), messages: [line], notes: [], status: FAIL_DEVICE})
    }
  }
  if kind == "sat" { info_name = f"{name} [SAT]" }
  protocol_name = if kind == "nvme" { "NVMe" } else if kind == "scsi" { "SCSI" } else { "ATA" }
  let members = device_members(clock_now, name, info_name, kind, protocol_name)

  if kind == "scsi" {
    if options.capabilities or options.xall or !options.logs.is_empty() or options.test != null or options.abort or options.smart_switch != null {
      gnu.error("-c, -x, -l, -t, -X and -s are not supported for SCSI devices: only identity, health and temperature are read")
      exit FAIL_COMMAND_LINE
    }
  }
  if kind in ["nvme", "scsi"] and (options.brief or options.badsum != "warn") {
    gnu.error("-f brief and -b change how ATA SMART data pages are shown and checked: they do not apply to NVMe or SCSI devices")
    exit FAIL_COMMAND_LINE
  }
  if kind == "nvme" {
    if options.smart_switch != null {
      gnu.error("-s is not supported for NVMe devices: they have no SMART enable switch")
      exit FAIL_COMMAND_LINE
    }
    for log in options.logs {
      if log not in ["error", "xerror", "selftest", "xselftest"] {
        gnu.error(f"-l {log} is not supported for NVMe devices: only the error and selftest logs exist")
        exit FAIL_COMMAND_LINE
      }
    }
  }
  var session: Session = {text: "", members: [], messages: [], notes: [], status: 0}
  if kind == "nvme" {
    session = nvme_session(options, fd, nsid_hint, clock_now)
  } else if kind == "scsi" {
    session = scsi_session(options, fd, clock_now)
  } else {
    session = ata_session(options, fd, cdb_size, clock_now)
  }
  finish(options, argv, uname.machine, uname.release, {text: leader + session.text, members: members + session.members, messages: session.messages, notes: session.notes, status: session.status})
}

# The members that name the run's clock and device, after the smartctl block.
pure device_members(now: Clock, name: Str, info_name: Str, kind: Str, protocol: Str) -> List[j.Member] {
  var members: List[j.Member] = [
    j.m("local_time", j.object([j.m("time_t", now.epoch), j.m("asctime", now.text)])),
  ]
  if kind != "" {
    members += [j.m("device", j.object([
      j.m("name", name),
      j.m("info_name", info_name),
      j.m("type", kind),
      j.m("protocol", protocol),
    ]))]
  }
  members
}
