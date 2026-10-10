##! The nvme command: NVMe device inspection over the `linux` storage
##! transport.
##!
##! Output layouts follow nvme-cli 2.x. Every device command is a read: Identify,
##! Get Log Page and Get Features go through `linux.nvme_admin`, which refuses
##! any other admin opcode. `device-self-test` and `set-feature` are the two
##! mutating commands and are narrowly validated; `format`, `fw-download` and
##! `fw-commit` are refused so no namespace is reformatted and no firmware is
##! written.
use gnu
use nvme_decode as dec

const VERSION_TEXT = "nvme version 2.16 (xsh)"

# Every sub-command name nvme-cli 2.16 prints in its command list, used to
# tell an unimplemented command from an unknown one.
const KNOWN_COMMANDS = "list list-subsys id-ctrl id-ns id-ns-granularity id-ns-lba-format list-ns list-ctrl nvm-id-ctrl nvm-id-ns nvm-id-ns-lba-format primary-ctrl-caps list-secondary cmdset-ind-id-ns ns-descs id-nvmset id-uuid id-iocs id-domain list-endgrp create-ns delete-ns attach-ns detach-ns get-ns-id get-log telemetry-log fw-log changed-ns-list-log smart-log ana-log error-log effects-log endurance-log predictable-lat-log pred-lat-event-agg-log persistent-event-log endurance-event-agg-log lba-status-log resv-notif-log boot-part-log phy-rx-eom-log get-feature device-self-test self-test-log supported-log-pages fid-support-effects-log mi-cmd-support-effects-log media-unit-stat-log supported-cap-config-log mgmt-addr-list-log rotational-media-info-log changed-alloc-ns-list-log dispersed-ns-participating-nss-log reachability-groups-log reachability-associations-log host-discovery-log ave-discovery-log pull-model-ddc-req-log set-feature set-property get-property format fw-commit fw-download admin-passthru io-passthru security-send security-recv get-lba-status capacity-mgmt resv-acquire resv-register resv-release resv-report dsm copy flush compare read write write-zeroes write-uncor verify sanitize sanitize-log reset subsystem-reset ns-rescan show-regs set-reg get-reg discover connect-all connect disconnect disconnect-all config gen-hostnqn show-hostnqn gen-dhchap-key check-dhchap-key gen-tls-key check-tls-key tls-key dir-receive dir-send virt-mgmt rpmb lockdown dim show-topology io-mgmt-recv io-mgmt-send nvme-mi-recv nvme-mi-send"

# Commands whose refusal is a safety decision rather than missing work.
const MUTATING_COMMANDS = ["format", "fw-commit", "fw-download", "sanitize", "create-ns", "delete-ns", "attach-ns", "detach-ns", "reset", "subsystem-reset", "write", "write-zeroes", "write-uncor", "dsm", "copy", "security-send", "set-property", "set-reg"]

const IMPLEMENTED = [
  "list|List all NVMe devices and namespaces on machine",
  "id-ctrl|Send NVMe Identify Controller",
  "id-ns|Send NVMe Identify Namespace, display structure",
  "list-ns|Send NVMe Identify List, display structure",
  "fw-log|Retrieve FW Log, show it",
  "smart-log|Retrieve SMART Log, show it",
  "error-log|Retrieve Error Log, show it",
  "get-feature|Get feature and show the resulting value",
  "device-self-test|Perform the necessary tests to observe the performance",
  "self-test-log|Retrieve the SELF-TEST Log, show it",
  "set-feature|Set a feature and show the resulting value",
  "version|Shows the program version",
  "help|Display this help",
]

const BOLD = "\x1b[1m"
const PLAIN = "\x1b[0m"

# Options every device command accepts: `SHORT:LONG` is a flag, `SHORT:LONG=TYPE`
# takes a value of that type (`word` 32 bits, `byte` 8 bits, `integer`,
# `string`).
const COMMON_OPTIONS = ["v:verbose", "o:output-format=string", "t:timeout=word", ":dry-run", ":no-retries", ":output-format-version=word", "h:help"]

# The log page identifiers and Identify controller selectors used here.
const OPCODE_GET_LOG_PAGE = 2
const OPCODE_IDENTIFY = 6
const OPCODE_GET_FEATURES = 10
const OPCODE_DEVICE_SELF_TEST = 20
const LOG_ERROR = 1
const LOG_FIRMWARE_SLOT = 3
const LOG_SMART = 2
const LOG_SELF_TEST = 6
const ALL_NAMESPACES = 4294967295
const LOG_CHUNK_BYTES = 4096

# Features `set-feature` may change: ones that take one dword of value and no
# data buffer and that a controller can always be put back from.
const SETTABLE_FEATURES = [4, 5, 6, 8, 10, 11, 15]

type Parsed = {values: Map[Str], operands: List[Str]}

# Everything a device command needs to issue admin commands and report.
type Session = {cmd: Str, fd: Int, json: Bool, verbose: Bool, dry_run: Bool, timeout_ms: Int}

type Reply = {result: Int, data: Bytes}

# Pre-rendered JSON text of one value; nested containers are rendered
# separately and indented by their parent.
type Json = Str

pure usage_text(cmd: Str) -> Str {
  match cmd {
    "list" => """Usage: nvme list <device> [OPTIONS]

Retrieve basic information for all NVMe namespaces

Options:
  [  --verbose, -v ]                    --- Increase output verbosity
  [  --output-format=<FMT>, -o <FMT> ]  --- Output format: normal|json|binary
  [  --timeout=<NUM>, -t <NUM> ]        --- timeout value, in milliseconds
  [  --dry-run ]                        --- show command instead of sending
  [  --no-retries ]                     --- disable retry logic on errors
                                            
  [  --output-format-version=<NUM> ]    --- output format version: 1|2
"""
    "id-ctrl" => """Usage: nvme id-ctrl <device> [OPTIONS]

Send an Identify Controller command to the given device and report
information about the specified controller in human-readable or binary
format. May also return vendor-specific controller attributes in hex-dump if
requested.

Options:
  [  --verbose, -v ]                    --- Increase output verbosity
  [  --output-format=<FMT>, -o <FMT> ]  --- Output format: normal|json|binary
  [  --vendor-specific, -V ]            --- dump binary vendor field
  [  --raw-binary, -b ]                 --- show identify in binary format
  [  --human-readable, -H ]             --- show identify in readable format
  [  --timeout=<NUM>, -t <NUM> ]        --- timeout value, in milliseconds
  [  --dry-run ]                        --- show command instead of sending
  [  --no-retries ]                     --- disable retry logic on errors
                                            
  [  --output-format-version=<NUM> ]    --- output format version: 1|2
"""
    "id-ns" => """Usage: nvme id-ns <device> [OPTIONS]

Send an Identify Namespace command to the given device, returns properties of
the specified namespace in either human-readable or binary format. Can also
return binary vendor-specific namespace attributes.

Options:
  [  --verbose, -v ]                    --- Increase output verbosity
  [  --output-format=<FMT>, -o <FMT> ]  --- Output format: normal|json|binary
  [  --namespace-id=<NUM>, -n <NUM> ]   --- identifier of desired namespace
  [  --force ]                          --- Return this namespace, even if
                                            not attached (1.2 devices only)
  [  --vendor-specific, -V ]            --- dump binary vendor fields
  [  --raw-binary, -b ]                 --- show identify in binary format
  [  --human-readable, -H ]             --- show identify in readable format
  [  --timeout=<NUM>, -t <NUM> ]        --- timeout value, in milliseconds
  [  --dry-run ]                        --- show command instead of sending
  [  --no-retries ]                     --- disable retry logic on errors
                                            
  [  --output-format-version=<NUM> ]    --- output format version: 1|2
"""
    "list-ns" => """Usage: nvme list-ns <device> [OPTIONS]

For the specified controller handle, show the namespace list in the
associated NVMe subsystem, optionally starting with a given nsid.

Options:
  [  --verbose, -v ]                    --- Increase output verbosity
  [  --output-format=<FMT>, -o <FMT> ]  --- Output format: normal|json|binary
  [  --namespace-id=<NUM>, -n <NUM> ]   --- first nsid returned list should
                                            start from
  [  --csi=<NUM>, -y <NUM> ]            --- I/O command set identifier
  [  --all, -a ]                        --- show all namespaces in the
                                            subsystem, whether attached or
                                            inactive
  [  --timeout=<NUM>, -t <NUM> ]        --- timeout value, in milliseconds
  [  --dry-run ]                        --- show command instead of sending
  [  --no-retries ]                     --- disable retry logic on errors
                                            
  [  --output-format-version=<NUM> ]    --- output format version: 1|2
"""
    "smart-log" => """Usage: nvme smart-log <device> [OPTIONS]

Retrieve SMART log for the given device (or optionally a namespace) in either
decoded format (default) or binary.

Options:
  [  --verbose, -v ]                    --- Increase output verbosity
  [  --output-format=<FMT>, -o <FMT> ]  --- Output format: normal|json|binary
  [  --namespace-id=<NUM>, -n <NUM> ]   --- (optional) desired namespace
  [  --raw-binary, -b ]                 --- output in binary format
  [  --human-readable, -H ]             --- show info in readable format
  [  --timeout=<NUM>, -t <NUM> ]        --- timeout value, in milliseconds
  [  --dry-run ]                        --- show command instead of sending
  [  --no-retries ]                     --- disable retry logic on errors
                                            
  [  --output-format-version=<NUM> ]    --- output format version: 1|2
"""
    "error-log" => """Usage: nvme error-log <device> [OPTIONS]

Retrieve specified number of error log entries from a given device in either
decoded format (default) or binary.

Options:
  [  --verbose, -v ]                    --- Increase output verbosity
  [  --output-format=<FMT>, -o <FMT> ]  --- Output format: normal|json|binary
  [  --log-entries=<NUM>, -e <NUM> ]    --- number of entries to retrieve
  [  --raw-binary, -b ]                 --- dump in binary format
  [  --timeout=<NUM>, -t <NUM> ]        --- timeout value, in milliseconds
  [  --dry-run ]                        --- show command instead of sending
  [  --no-retries ]                     --- disable retry logic on errors
                                            
  [  --output-format-version=<NUM> ]    --- output format version: 1|2
"""
    "self-test-log" => """Usage: nvme self-test-log <device> [OPTIONS]

Retrieve the self-test log for the given device and given test (or optionally
a namespace) in either decoded format (default) or binary.

Options:
  [  --verbose, -v ]                    --- Increase output verbosity
  [  --output-format=<FMT>, -o <FMT> ]  --- Output format: normal|json|binary
  [  --dst-entries=<NUM>, -e <NUM> ]    --- Indicate how many DST log entries
                                            to be retrieved, by default all
                                            the 20 entries will be retrieved
  [  --timeout=<NUM>, -t <NUM> ]        --- timeout value, in milliseconds
  [  --dry-run ]                        --- show command instead of sending
  [  --no-retries ]                     --- disable retry logic on errors
                                            
  [  --output-format-version=<NUM> ]    --- output format version: 1|2
"""
    "fw-log" => """Usage: nvme fw-log <device> [OPTIONS]

Retrieve the firmware log for the specified device in either decoded format
(default) or binary.

Options:
  [  --verbose, -v ]                    --- Increase output verbosity
  [  --output-format=<FMT>, -o <FMT> ]  --- Output format: normal|json|binary
  [  --raw-binary, -b ]                 --- use binary output
  [  --timeout=<NUM>, -t <NUM> ]        --- timeout value, in milliseconds
  [  --dry-run ]                        --- show command instead of sending
  [  --no-retries ]                     --- disable retry logic on errors
                                            
  [  --output-format-version=<NUM> ]    --- output format version: 1|2
"""
    "get-feature" => """Usage: nvme get-feature <device> [OPTIONS]

Read operating parameters of the specified controller. Operating parameters
are grouped and identified by Feature Identifiers; each Feature Identifier
contains one or more attributes that may affect behavior of the feature.
Each Feature has three possible settings: default, saveable, and current. If
a Feature is saveable, it may be modified by set-feature. Default values are
vendor-specific and not changeable. Use set-feature to change saveable
Features.

Options:
  [  --verbose, -v ]                    --- Increase output verbosity
  [  --output-format=<FMT>, -o <FMT> ]  --- Output format: normal|json|binary
  [  --feature-id=<NUM>, -f <NUM> ]     --- feature identifier
  [  --namespace-id=<NUM>, -n <NUM> ]   --- identifier of desired namespace
  [  --sel=<NUM>, -s <NUM> ]            --- [0-3]:
                                            current/default/saved/supported
  [  --data-len=<NUM>, -l <NUM> ]       --- buffer len (if) data is sent or
                                            received
  [  --raw-binary, -b ]                 --- show feature in binary format
  [  --cdw11=<NUM>, -c <NUM> ]          --- feature specific dword 11
  [  --uuid-index=<NUM>, -U <NUM> ]     --- specify uuid index
  [  --human-readable, -H ]             --- show feature in readable format
  [  --changed, -C ]                    --- show feature changed
  [  --timeout=<NUM>, -t <NUM> ]        --- timeout value, in milliseconds
  [  --dry-run ]                        --- show command instead of sending
  [  --no-retries ]                     --- disable retry logic on errors
                                            
  [  --output-format-version=<NUM> ]    --- output format version: 1|2
"""
    "set-feature" => """Usage: nvme set-feature <device> [OPTIONS]

Modify the saveable or changeable current operating parameters of the
controller. Operating parameters are grouped and identified by
FeatureIdentifiers. Feature settings can be applied to the entirecontroller
and all associated namespaces, or to only a fewnamespace(s) associated with
the controller. Default valuesfor each Feature are vendor-specific and may
not be modified.Use get-feature to determine which Features are supported
bythe controller and are saveable/changeable.

Options:
  [  --verbose, -v ]                    --- Increase output verbosity
  [  --output-format=<FMT>, -o <FMT> ]  --- Output format: normal|json|binary
  [  --namespace-id=<NUM>, -n <NUM> ]   --- desired namespace
  [  --feature-id=<NUM>, -f <NUM> ]     --- feature identifier (required)
  [  --value=<IONUM>, -V <IONUM> ]      --- new value of feature (required)
  [  --cdw12=<NUM>, -c <NUM> ]          --- feature cdw12, if used
  [  --uuid-index=<NUM>, -U <NUM> ]     --- specify uuid index
  [  --data-len=<NUM>, -l <NUM> ]       --- buffer len (if) data is sent or
                                            received
  [  --data=<FILE>, -d <FILE> ]         --- optional file for feature data
                                            (default stdin)
  [  --save, -s ]                       --- specifies that the controller
                                            shall save the attribute
  [  --timeout=<NUM>, -t <NUM> ]        --- timeout value, in milliseconds
  [  --dry-run ]                        --- show command instead of sending
  [  --no-retries ]                     --- disable retry logic on errors
                                            
  [  --output-format-version=<NUM> ]    --- output format version: 1|2
"""
    "device-self-test" => """Usage: nvme device-self-test <device> [OPTIONS]

Implementing the device self-test feature which provides the necessary log to
determine the state of the device

Options:
  [  --verbose, -v ]                    --- Increase output verbosity
  [  --output-format=<FMT>, -o <FMT> ]  --- Output format: normal|json|binary
  [  --namespace-id=<NUM>, -n <NUM> ]   --- Indicate the namespace in which
                                            the device self-test has to be
                                            carried out
  [  --self-test-code=<NUM>, -s <NUM> ] --- This field specifies the action
                                            taken by the device self-test
                                            command :
                                            0h Show current state of device
                                            self-test operation
                                            1h Start a short device
                                            self-test operation
                                            2h Start a extended device
                                            self-test operation
                                            3h Start a Host-Initiated
                                            Refresh operation
                                            eh Start a vendor specific
                                            device self-test operation
                                            fh Abort the device self-test
                                            operation
  [  --wait, -w ]                       --- Wait for the test to finish
  [  --timeout=<NUM>, -t <NUM> ]        --- timeout value, in milliseconds
  [  --dry-run ]                        --- show command instead of sending
  [  --no-retries ]                     --- disable retry logic on errors
                                            
  [  --output-format-version=<NUM> ]    --- output format version: 1|2
"""
    else => ""
  }
}

# The command's usage text, without a final newline, with the two headings in
# bold as nvme-cli prints them whether or not stdout is a terminal.
pure usage_with_headings(cmd: Str) -> Str {
  var out: List[Str] = []
  for line in usage_text(cmd).lines() {
    if line.starts_with("Usage: ") or line == "Options:" {
      out += [BOLD + line + PLAIN]
    } else {
      out += [line]
    }
  }
  out.join("\n")
}

pure main_usage() -> Str {
  var lines = [
    "nvme-2.16",
    "usage: nvme <command> [<device>] [<args>]",
    "",
    "The '<device>' may be either an NVMe character device (ex: /dev/nvme0), an",
    "nvme block device (ex: /dev/nvme0n1), or a mctp address in the form",
    "mctp:<net>,<eid>[:ctrl-id]",
    "",
    "The following are all implemented sub-commands:",
  ]
  for entry in IMPLEMENTED {
    let parts = entry.split("|")
    lines += [f"  {tui.right_pad(parts[0], 36)} {parts[1]}"]
  }
  lines += ["", "See 'nvme help <command>' for more information on a specific command"]
  lines.join("\n")
}

proc usage_failure(cmd: Str, message: Str) [process, env, io] -> Unit {
  eprint f"{cmd}: {message}"
  eprint usage_with_headings(cmd)
  exit 1
}

proc fail(as_json: Bool, message: Str) [process, env, io] -> Unit {
  if as_json {
    gnu.write_text("{\n  \"error\":" + json_string(message) + "\n}\n")
  } else {
    eprint $message
  }
  exit 1
}

pure json_string(text: Str) -> Json {
  json.encode(text) ?? "\"\""
}

# A JSON object in json-c's pretty layout: one `"key":value` member per line
# and containers nested two spaces deeper. `members` holds [key, rendered].
pure json_object(members: List[List[Str]]) -> Json {
  if members.is_empty() { return "{}" }
  let lines = ["  " + json_string(member[0]) + ":" + member[1].replace("\n", with: "\n  ") for member in members]
  "{\n" + lines.join(",\n") + "\n}"
}

pure json_array(items: List[Str]) -> Json {
  if items.is_empty() { return "[]" }
  let lines = ["  " + item.replace("\n", with: "\n  ") for item in items]
  "[\n" + lines.join(",\n") + "\n]"
}

# strtoul-style number: optional blanks and sign, `0x` hex, leading-zero
# octal, then as many digits as there are; null when no digit is present.
pure c_number(text: Str) -> Int? {
  let length = text.byte_len()
  var index = 0
  while index < length and (text.byte_at(index) == 32 or ((text.byte_at(index) ?? 0) >= 9 and (text.byte_at(index) ?? 0) <= 13)) {
    index += 1
  }
  var negative = false
  if index < length and text.byte_at(index) == 45 {
    negative = true
    index += 1
  } else if index < length and text.byte_at(index) == 43 {
    index += 1
  }
  var base = 10
  if index + 2 < length + 0 and text.byte_at(index) == 48 and (text.byte_at(index + 1) == 120 or text.byte_at(index + 1) == 88) and digit_value(text.byte_at(index + 2) ?? 0) >= 0 and digit_value(text.byte_at(index + 2) ?? 0) < 16 {
    base = 16
    index += 2
  } else if index < length and text.byte_at(index) == 48 {
    base = 8
  }
  var value = 0
  var digits = 0
  while index < length {
    let digit = digit_value(text.byte_at(index) ?? 0)
    if digit < 0 or digit >= base { break }
    if value <= 72057594037927935 { value = value * base + digit } else { value = 1152921504606846975 }
    digits += 1
    index += 1
  }
  return null when digits == 0

  if negative { -value } else { value }
}

pure digit_value(byte: Int) -> Int {
  if byte >= 48 and byte <= 57 { return byte - 48 }
  if byte >= 97 and byte <= 102 { return byte - 87 }
  if byte >= 65 and byte <= 70 { return byte - 55 }
  -1
}

# Parses `argv` like getopt_long with argument permutation: long options may
# be abbreviated, short flags cluster, and a value option takes the rest of
# its word or the next word. Numbers are checked as they are read, in command
# line order, and stored in decimal.
proc parse(cmd: Str, own: List[Str], argv: List[Str]) [process, env, io] -> Parsed {
  let specs = own + COMMON_OPTIONS
  var values: Map[Str] = {}
  var operands: List[Str] = []
  var index = 0
  var ended = false
  while index < argv.len() {
    let word = argv[index]
    index += 1
    if ended or ! word.starts_with("-") or word == "-" {
      operands += [word]
      continue
    }
    if word == "--" {
      ended = true
      continue
    }
    if word.starts_with("--") {
      let body = word.byte_slice(2)
      let split = body.find("=")
      let option = if split == null { body } else { body.byte_slice(0, split ?? 0) }
      var matches: List[Str] = []
      for spec in specs {
        let long = long_name(spec)
        if long == option {
          matches = [spec]
          break
        }
        if long.starts_with(option) { matches += [spec] }
      }
      if matches.is_empty() { usage_failure(cmd, f"unrecognized option: {option}") }
      if matches.len() > 1 { usage_failure(cmd, f"option is ambiguous: {option}") }
      let spec = matches[0]
      let long = long_name(spec)
      let kind = value_kind(spec)
      if kind == "" {
        if split != null { usage_failure(cmd, f"option doesn't allow an argument: {long}") }
        values = values.set(long, "1")
      } else {
        var text = ""
        if split != null {
          text = body.byte_slice((split ?? 0) + 1)
        } else {
          if index >= argv.len() { usage_failure(cmd, f"option requires an argument: {long}") }
          text = argv[index]
          index += 1
        }
        values = values.set(long, checked(long, kind, text))
      }
      continue
    }
    var position = 1
    let letters = word.byte_len()
    while position < letters {
      let letter = word.byte_slice(position, length: 1)
      position += 1
      var found: Str? = null
      for spec in specs {
        if short_name(spec) == letter { found = spec }
      }
      if found == null { usage_failure(cmd, f"unrecognized option: {letter}") }
      let spec = found ?? ""
      let long = long_name(spec)
      let kind = value_kind(spec)
      if kind == "" {
        values = values.set(long, "1")
        continue
      }
      var text = ""
      if position < letters {
        text = word.byte_slice(position)
        position = letters
      } else {
        if index >= argv.len() { usage_failure(cmd, f"option requires an argument: {letter}") }
        text = argv[index]
        index += 1
      }
      values = values.set(long, checked(long, kind, text))
    }
  }
  if "help" in values {
    gnu.write_text(usage_with_headings(cmd) + "\n")
    exit 1
  }
  {values: values, operands: operands}
}

pure long_name(spec: Str) -> Str {
  let after = spec.byte_slice((spec.find(":") ?? 0) + 1)
  after.split("=")[0]
}

pure short_name(spec: Str) -> Str {
  spec.byte_slice(0, spec.find(":") ?? 0)
}

# The value type of an option spec, or "" for a flag.
pure value_kind(spec: Str) -> Str {
  let parts = spec.split("=")
  if parts.len() > 1 { parts[1] } else { "" }
}

# Validates one option value as nvme-cli's argument parser does and returns
# it as decimal text for numbers.
proc checked(long: Str, kind: Str, text: Str) [process, env, io] -> Str {
  if kind == "string" { return text }
  let parsed = c_number(text)
  let name = if kind == "byte" { "byte" } else if kind == "word" { "word" } else { "integer" }
  if parsed == null {
    eprint f"Expected {name} argument for '{long}' but got '{text}'!"
    exit 1
  }
  var amount = parsed ?? 0
  if kind == "byte" {
    if amount < 0 or amount > 255 {
      eprint f"Expected {name} argument for '{long}' but got '{text}'!"
      exit 1
    }
  } else if kind == "word" {
    amount = (amount % 4294967296 + 4294967296) % 4294967296
  }
  f"{amount}"
}

pure has(parsed: Parsed, long: Str) -> Bool {
  long in parsed.values
}

pure number(parsed: Parsed, long: Str, fallback: Int) -> Int {
  (parsed.values.get(long) ?? "").parse_int() ?? fallback
}

pure hex(value: Int) -> Str {
  if value == 0 { "0" } else { "0x" + dec.hex_digits(value) }
}

pure hex_field(field: dec.Field) -> Str {
  if field.hex == "0" { "0" } else { "0x" + field.hex }
}

# The device node name nvme-cli prints: the path without its directory.
pure device_name(node: Str) -> Str {
  let parts = node.split("/")
  parts[parts.len() - 1]
}

# Opens the device named by the single operand, checking it is a block or
# character device first as nvme-cli does. A bare name such as `nvme0` is a
# node in /dev.
proc open_device(cmd: Str, parsed: Parsed) [fs, process, env, io, error] -> Int {
  if parsed.operands.is_empty() { usage_failure(cmd, "Invalid argument") }
  if parsed.operands.len() > 1 { usage_failure(cmd, f"unexpected extra argument: {parsed.operands[1]}") }
  let name = parsed.operands[0]
  let node = if name.find("/") == null { fp"/dev/{name}" } else { fp"{name}" }
  match fs.stat(node) {
    Err(failure) => device_failure(cmd, f"{name}: {gnu.strerror(failure)}")
    Ok(info) => {
      if info.kind != "char" and info.kind != "block" {
        device_failure(cmd, f"{name} is not a block or character device")
      }
    }
  }
  match unix.open_fd(node) {
    Ok(fd) => fd
    Err(failure) => {
      device_failure(cmd, f"{name}: {gnu.strerror(failure)}")
      -1
    }
  }
}

# A device that cannot be used is reported by itself, followed by the
# command's usage.
proc device_failure(cmd: Str, message: Str) [process, env, io] -> Unit {
  eprint $message
  eprint usage_with_headings(cmd)
  exit 1
}

# Chooses normal, json or binary output. `-b` and `-o binary` both mean raw
# bytes; any other name is refused. Checked after the device opens, like
# nvme-cli.
proc output_format(parsed: Parsed, binary_allowed: Bool) [process, env, io] -> Str {
  let named = parsed.values.get("output-format") ?? "normal"
  var format = named
  if has(parsed, "raw-binary") { format = "binary" }
  if format != "normal" and format != "json" and format != "binary" {
    eprint "Invalid output format"
    exit 1
  }
  if format == "binary" and ! binary_allowed {
    eprint "Invalid output format"
    exit 1
  }
  if has(parsed, "output-format-version") and number(parsed, "output-format-version", 1) != 1 {
    eprint f"--output-format-version={number(parsed, "output-format-version", 1)} is not supported: only version 1 is produced"
    exit 1
  }
  format
}

pure make_session(cmd: Str, fd: Int, format: Str, parsed: Parsed) -> Session {
  {
    cmd: cmd, fd: fd, json: format == "json", verbose: has(parsed, "verbose"),
    dry_run: has(parsed, "dry-run"), timeout_ms: number(parsed, "timeout", 0),
  }
}

# Issues one admin command and returns its completion dword and data. It
# reads from the device unless `change` is set, which only the commands that
# modify a device (`device-self-test`, `set-feature`) do. A failing ioctl is reported with the
# strerror text under `what`; a device status as nvme-cli's `NVMe status`.
# `--dry-run` sends nothing and ends the command.
proc admin(dev: Session, what: Str, opcode: Int, nsid: Int, cdw10: Int, length: Int, cdw11: Int = 0, cdw12: Int = 0, cdw13: Int = 0, cdw14: Int = 0, change: Bool = false) [process, env, io, error] -> Reply {
  if dev.verbose {
    eprint f"{dev.cmd}: opcode {hex(opcode)} nsid {hex(nsid)} cdw10 {hex(cdw10)} cdw11 {hex(cdw11)} cdw12 {hex(cdw12)} cdw13 {hex(cdw13)} cdw14 {hex(cdw14)} data_len {length}"
  }
  if dev.dry_run { exit 0 }
  let outcome = if change {
    linux.nvme_admin_command(dev.fd, opcode, length, nsid: nsid, cdw10: cdw10, cdw11: cdw11, cdw12: cdw12, cdw13: cdw13, cdw14: cdw14, timeout_ms: dev.timeout_ms)
  } else {
    linux.nvme_admin(dev.fd, opcode, length, nsid: nsid, cdw10: cdw10, cdw11: cdw11, cdw12: cdw12, cdw13: cdw13, cdw14: cdw14, timeout_ms: dev.timeout_ms)
  }
  match outcome {
    Err(failure) => {
      fail(dev.json, f"{what}: {gnu.strerror(failure)}")
      {result: 0, data: b""}
    }
    Ok(reply) => {
      if reply.status != 0 {
        let code = reply.status % 2048
        fail(dev.json, f"NVMe status: {dec.status_text(code)}({hex(reply.status)})")
      }
      if reply.data.len() != length {
        fail(dev.json, f"{what}: device returned {reply.data.len()} of {length} bytes")
      }
      {result: reply.result, data: reply.data}
    }
  }
}

# One Get Log Page covering `length` bytes at `nsid`, in transfers of at most
# 4 KiB with the log page offset carried in dwords 12 and 13.
proc get_log(dev: Session, what: Str, log_id: Int, nsid: Int, length: Int) [process, env, io, error] -> Bytes {
  var chunks: List[Bytes] = []
  var offset = 0
  while offset < length {
    let size = if length - offset > LOG_CHUNK_BYTES { LOG_CHUNK_BYTES } else { length - offset }
    let dwords = size / 4 - 1
    let cdw10 = dwords % 65536 * 65536 + log_id
    let reply = admin(dev, what, OPCODE_GET_LOG_PAGE, nsid, cdw10, size, cdw11: dwords / 65536, cdw12: offset % 4294967296, cdw13: offset / 4294967296)
    chunks += [reply.data]
    offset += size
  }
  bytes.concat(chunks)
}

# The namespace for commands that need one: `-n`, otherwise the namespace id
# of the opened block device.
proc namespace_of(dev: Session, parsed: Parsed) [process, env, io, error] -> Int {
  if has(parsed, "namespace-id") { return number(parsed, "namespace-id", 0) }
  if dev.dry_run { return 0 }
  match linux.nvme_namespace_id(dev.fd) {
    Ok(nsid) => nsid
    Err(failure) => {
      fail(dev.json, f"get-namespace-id: {gnu.strerror(failure)}")
      0
    }
  }
}

# Pads a label to `width` and joins it to its value with ` : `.
pure label(name: Str, width: Int, text: Str) -> Str {
  f"{tui.right_pad(name, width)} : {text}"
}

# One field's bits in the human-readable layout: entries are
# `HIGH LOW|TEXT WHEN 0|TEXT WHEN 1`; a bit field wider than one bit uses its
# first text, an empty text for zero hides a reserved field that is zero.
# `%P` in a text is replaced by two to the power of the field value.
pure bit_lines(value: Int, table: List[Str], prefix: Str, between: Str) -> List[Str] {
  var out: List[Str] = []
  for entry in table {
    let parts = entry.split("|")
    let range = parts[0].split(" ")
    let high = range[0].parse_int() ?? 0
    let low = range[1].parse_int() ?? 0
    var shifted = value
    for _ in range(low) { shifted = shifted / 2 }
    var span = 1
    for _ in range(high - low + 1) { span = span * 2 }
    let field = shifted % span
    var text = if field == 0 { parts[1] } else if parts.len() > 2 and high == low { parts[2] } else { parts[1] }
    if text == "" { continue }
    var power = 1
    for _ in range(field) { power = power * 2 }
    text = text.replace("%P", with: f"{power}")
    out += [f"{prefix}[{high}:{low}]{between}: {hex(field)}\t{text}"]
  }
  out
}

pure controller_detail(name: Str, value: Int) -> List[Str] {
  match name {
    "cmic" => bit_lines(value, [
      "3 3|ANA Reporting Not Supported|ANA Reporting Supported",
      "2 2|Controller is associated with a PCI Function|Controller is associated with an SR-IOV Virtual Function",
      "1 1|NVM subsystem may have only one Controller|NVM subsystem may have two or more Controllers",
      "0 0|NVM subsystem has only one port|NVM subsystem may have more than one port",
    ], "  ", " ")
    "oacs" => bit_lines(value, [
      "10 10|Command and Feature Lockdown Not Supported|Command and Feature Lockdown Supported",
      "9 9|Get LBA Status Not Supported|Get LBA Status Supported",
      "8 8|Doorbell Buffer Config Not Supported|Doorbell Buffer Config Supported",
      "7 7|Virtualization Management Not Supported|Virtualization Management Supported",
      "6 6|NVMe-MI Send and Receive Not Supported|NVMe-MI Send and Receive Supported",
      "5 5|Directives Not Supported|Directives Supported",
      "4 4|Device Self-test Not Supported|Device Self-test Supported",
      "3 3|NS Management and Attachment Not Supported|NS Management and Attachment Supported",
      "2 2|FW Commit and Download Not Supported|FW Commit and Download Supported",
      "1 1|Format NVM Not Supported|Format NVM Supported",
      "0 0|Security Send and Receive Not Supported|Security Send and Receive Supported",
    ], "  ", " ")
    "frmw" => bit_lines(value, [
      "7 5||Reserved",
      "4 4|Firmware Activate Without Reset Not Supported|Firmware Activate Without Reset Supported",
      "3 1|Number of Firmware Slots|Number of Firmware Slots",
      "0 0|Firmware Slot 1 Read/Write|Firmware Slot 1 Read-Only",
    ], "  ", " ")
    "lpa" => bit_lines(value, [
      "7 5||Reserved",
      "4 4|Persistent Event Log Not Supported|Persistent Event Log Supported",
      "3 3|Telemetry Log Page Not Supported|Telemetry Log Page Supported",
      "2 2|Extended Data for Get Log Page Not Supported|Extended Data for Get Log Page Supported",
      "1 1|Command Effects Log Page Not Supported|Command Effects Log Page Supported",
      "0 0|SMART/Health Log Page per NS Not Supported|SMART/Health Log Page per NS Supported",
    ], "  ", " ")
    "avscc" => bit_lines(value, [
      "0 0|Admin Vendor Specific Commands uses Vendor Specific Format|Admin Vendor Specific Commands uses NVMe Format",
    ], "  ", " ")
    "apsta" => bit_lines(value, [
      "0 0|Autonomous Power State Transitions Not Supported|Autonomous Power State Transitions Supported",
    ], "  ", " ")
    "hctma" => bit_lines(value, [
      "0 0|Host Controlled Thermal Management Not Supported|Host Controlled Thermal Management Supported",
    ], "  ", " ")
    "sanicap" => bit_lines(value, [
      "2 2|Overwrite Sanitize Operation Not Supported|Overwrite Sanitize Operation Supported",
      "1 1|Block Erase Sanitize Operation Not Supported|Block Erase Sanitize Operation Supported",
      "0 0|Crypto Erase Sanitize Operation Not Supported|Crypto Erase Sanitize Operation Supported",
    ], "  ", " ")
    "sqes" => bit_lines(value, [
      "7 4|Max SQ Entry Size (%P)|Max SQ Entry Size (%P)",
      "3 0|Min SQ Entry Size (%P)|Min SQ Entry Size (%P)",
    ], "  ", " ")
    "cqes" => bit_lines(value, [
      "7 4|Max CQ Entry Size (%P)|Max CQ Entry Size (%P)",
      "3 0|Min CQ Entry Size (%P)|Min CQ Entry Size (%P)",
    ], "  ", " ")
    "oncs" => bit_lines(value, [
      "8 8|Copy Not Supported|Copy Supported",
      "7 7|Verify Not Supported|Verify Supported",
      "6 6|Timestamp Not Supported|Timestamp Supported",
      "5 5|Reservations Not Supported|Reservations Supported",
      "4 4|Save and Select Not Supported|Save and Select Supported",
      "3 3|Write Zeroes Not Supported|Write Zeroes Supported",
      "2 2|Data Set Management Not Supported|Data Set Management Supported",
      "1 1|Write Uncorrectable Not Supported|Write Uncorrectable Supported",
      "0 0|Compare Not Supported|Compare Supported",
    ], "  ", " ")
    "fuses" => bit_lines(value, [
      "0 0|Fused Compare and Write Not Supported|Fused Compare and Write Supported",
    ], "  ", " ")
    "fna" => bit_lines(value, [
      "2 2|Crypto Erase Not Supported as part of Secure Erase|Crypto Erase Supported as part of Secure Erase",
      "1 1|Format Applies to Single Namespace(s)|Format Applies to All Namespace(s)",
      "0 0|Secure Erase Applies to Single Namespace(s)|Secure Erase Applies to All Namespace(s)",
    ], "  ", " ")
    "vwc" => bit_lines(value, [
      "0 0|Volatile Write Cache Not Present|Volatile Write Cache Present",
    ], "  ", " ")
    else => []
  }
}

pure namespace_detail(name: Str, value: Int) -> List[Str] {
  match name {
    "nsfeat" => bit_lines(value, [
      "7 5||Reserved",
      "4 4|NPWG, NPWA, NPDG, NPDA, and NOWS are Not Supported|NPWG, NPWA, NPDG, NPDA, and NOWS are Supported",
      "3 3|NGUID and EUI64 fields if non-zero, Reused|NGUID and EUI64 fields if non-zero, Never Reused",
      "2 2|Deallocated or Unwritten Logical Block error Not Supported|Deallocated or Unwritten Logical Block error Supported",
      "1 1|Namespace uses AWUN, AWUPF, and ACWU|Namespace uses NAWUN, NAWUPF, and NACWU",
      "0 0|Thin Provisioning Not Supported|Thin Provisioning Supported",
    ], "  ", " ")
    "flbas" => bit_lines(value, [
      "6 5|Most significant 2 bits of Current LBA Format Selected|Most significant 2 bits of Current LBA Format Selected",
      "4 4|Metadata Transferred in Separate Buffer|Metadata Transferred at End of Data LBA",
      "3 0|Least significant 4 bits of Current LBA Format Selected|Least significant 4 bits of Current LBA Format Selected",
    ], "  ", " ")
    "mc" => bit_lines(value, [
      "1 1|Metadata Pointer Not Supported|Metadata Pointer Supported",
      "0 0|Metadata as Part of Extended Data LBA Not Supported|Metadata as Part of Extended Data LBA Supported",
    ], "  ", " ")
    "dpc" => bit_lines(value, [
      "4 4|Protection Information Transferred as Last Eight Bytes of Metadata Not Supported|Protection Information Transferred as Last Eight Bytes of Metadata Supported",
      "3 3|Protection Information Transferred as First Eight Bytes of Metadata Not Supported|Protection Information Transferred as First Eight Bytes of Metadata Supported",
      "2 2|Protection Information Type 3 Not Supported|Protection Information Type 3 Supported",
      "1 1|Protection Information Type 2 Not Supported|Protection Information Type 2 Supported",
      "0 0|Protection Information Type 1 Not Supported|Protection Information Type 1 Supported",
    ], "  ", " ")
    "dps" => bit_lines(value, [
      "3 3|Protection Information is Transferred as Last Eight Bytes of Metadata|Protection Information is Transferred as First Eight Bytes of Metadata",
      "2 0|Protection Information Type|Protection Information Type",
    ], "  ", " ")
    "nmic" => bit_lines(value, [
      "0 0|Namespace Multipath Not Capable|Namespace Multipath Capable",
    ], "  ", " ")
    "rescap" => bit_lines(value, [
      "7 7|Ignore Existing Key Not Supported|Ignore Existing Key Used as defined in revision 1.2.1 or earlier",
      "6 6|Exclusive Access - All Registrants Not Supported|Exclusive Access - All Registrants Supported",
      "5 5|Write Exclusive - All Registrants Not Supported|Write Exclusive - All Registrants Supported",
      "4 4|Exclusive Access - Registrants Only Not Supported|Exclusive Access - Registrants Only Supported",
      "3 3|Write Exclusive - Registrants Only Not Supported|Write Exclusive - Registrants Only Supported",
      "2 2|Exclusive Access Not Supported|Exclusive Access Supported",
      "1 1|Write Exclusive Not Supported|Write Exclusive Supported",
      "0 0|Persist Through Power Loss Not Supported|Persist Through Power Loss Supported",
    ], "  ", " ")
    "fpi" => bit_lines(value, [
      "7 7|Format Progress Indicator Not Supported|Format Progress Indicator Supported",
      "6 0|Format Progress Indicator (Remaining 0%)|Format Progress Indicator (Remaining 0%)",
    ], "  ", " ")
    "dlfeat" => bit_lines(value, [
      "4 4|Guard Field of Deallocated Logical Blocks is set to 0xFFFF|Guard Field of Deallocated Logical Blocks is set to CRC of The Value in the Read Value field",
      "3 3|Deallocate Bit in the Write Zeroes Command is Not Supported|Deallocate Bit in the Write Zeroes Command is Supported",
      "2 0|Bytes Read From a Deallocated Logical Block and its Metadata|Bytes Read From a Deallocated Logical Block and its Metadata",
    ], "  ", " ")
    else => []
  }
}

# The hexdump nvme-cli prints for vendor-specific regions: a column header,
# then sixteen bytes per row with their printable characters in quotes.
pure hex_dump(data: Bytes) -> List[Str] {
  var lines: List[Str] = ["     " + [f"{tui.left_pad(dec.hex_digits(column), 3)}" for column in range(16)].join("")]
  let total = data.len()
  var row = 0
  while row < total {
    var line = tui.left_pad(dec.hex_digits(row), 4).replace(" ", with: "0") + ":"
    var shown = ""
    var column = 0
    while column < 16 and row + column < total {
      let byte = dec.u8(data, row + column)
      line += " " + (if byte < 16 { "0" } else { "" }) + dec.hex_digits(byte)
      shown += if byte == 32 { "." } else { dec.printable_char(byte) }
      column += 1
    }
    if column < 16 {
      line += " " + tui.right_pad("", (16 - column) * 3)
    }
    lines += [line + " \"" + shown + "\""]
    row += 16
  }
  lines
}

pure power_text(value: Int, scale: Int) -> Str {
  if scale == 1 {
    let fraction = f"{value % 10000}"
    f"{value / 10000}." + "0000".byte_slice(0, length: 4 - fraction.byte_len()) + fraction + "W"
  } else if scale == 2 {
    let fraction = f"{value % 100}"
    f"{value / 100}." + "00".byte_slice(0, length: 2 - fraction.byte_len()) + fraction + "W"
  } else {
    "-"
  }
}

# The maximum power of a power state: centiwatts, or 0.0001 W when MXPS is set.
pure max_power_text(state: dec.PowerState) -> Str {
  if state.max_scale == 1 { power_text(state.max_power, 1) } else { power_text(state.max_power, 2) }
}

pure power_state_lines(states: List[dec.PowerState]) -> List[Str] {
  var lines: List[Str] = []
  for index in range(states.len()) {
    let state = states[index]
    let kind = if state.non_operational { "non-operational" } else { "operational" }
    lines += [f"ps {tui.left_pad(f"{index}", 4)} : mp:{max_power_text(state)} {kind} enlat:{state.entry_latency} exlat:{state.exit_latency} rrt:{state.read_throughput} rrl:{state.read_latency}"]
    lines += [f"          rwt:{state.write_throughput} rwl:{state.write_latency} idle_power:{power_text(state.idle_power, state.idle_scale)} active_power:{power_text(state.active_power, state.active_scale)}"]
    let workload = if state.active_scale == 0 { "-" } else { f"{state.workload}" }
    lines += [f"          active_power_workload:{workload}"]
  }
  lines
}

# Everything a device command has resolved before it sends anything.
type Begun = {parsed: Parsed, dev: Session, format: Str}

proc begin(cmd: Str, own: List[Str], argv: List[Str], binary_allowed: Bool = true) [fs, process, env, io, error] -> Begun {
  let parsed = parse(cmd, own, argv)
  let fd = open_device(cmd, parsed)
  let format = output_format(parsed, binary_allowed)
  {parsed: parsed, dev: make_session(cmd, fd, format, parsed), format: format}
}

pure hex_padded(value: Int, width: Int) -> Str {
  let digits = dec.hex_digits(value)
  "0x" + "00000000".byte_slice(0, length: width - digits.byte_len()) + digits
}

pure json_number(value: Int) -> Json {
  f"{value}"
}

pure field_json(item: dec.Field) -> Json {
  if item.style == "s" or item.style == "hexstr" { json_string(item.text) } else { item.decimal }
}

proc id_ctrl(argv: List[Str]) [fs, process, env, io, error] {
  let begun = begin("id-ctrl", ["V:vendor-specific", "b:raw-binary", "H:human-readable"], argv)
  let dev = begun.dev
  let vendor = has(begun.parsed, "vendor-specific")
  if vendor and begun.format == "json" { fail(true, "id-ctrl: --vendor-specific is not available in json output") }
  let data = admin(dev, "identify controller", OPCODE_IDENTIFY, 0, 1, 4096).data
  if begun.format == "binary" {
    gnu.write_bytes(data)
    return
  }
  let fields = dec.decode(data, dec.CONTROLLER_LAYOUT)
  var count = dec.field_value(fields, "npss") + 1
  if count > 32 { count = 32 }
  let states = dec.power_states(data, count)
  if begun.format == "json" {
    var members: List[List[Str]] = [[item.name, field_json(item)] for item in fields]
    let descriptors = [json_object([
      ["max_power", json_number(state.max_power)],
      ["flags", json_number(state.max_scale + 2 * (if state.non_operational { 1 } else { 0 }))],
      ["entry_lat", json_number(state.entry_latency)],
      ["exit_lat", json_number(state.exit_latency)],
      ["read_tput", json_number(state.read_throughput)],
      ["read_lat", json_number(state.read_latency)],
      ["write_tput", json_number(state.write_throughput)],
      ["write_lat", json_number(state.write_latency)],
      ["idle_power", json_number(state.idle_power)],
      ["idle_scale", json_number(state.idle_scale)],
      ["active_power", json_number(state.active_power)],
      ["active_scale", json_number(state.active_scale)],
      ["active_workload", json_number(state.workload)],
    ]) for state in states]
    members += [["psds", json_array(descriptors)]]
    gnu.write_text(json_object(members) + "\n")
    return
  }
  let human = has(begun.parsed, "human-readable")
  var lines = ["NVME Identify Controller:"]
  for item in fields {
    lines += [label(item.name, 9, dec.display(item))]
    if human { lines += controller_detail(item.name, item.value) }
  }
  lines += power_state_lines(states)
  if vendor { lines += ["vs[]:"] + hex_dump(data.slice(3072, length: 1024)) }
  gnu.write_text(lines.join("\n") + "\n")
}

pure ns_label(name: Str, text: Str) -> Str {
  f"{tui.right_pad(name, 8)}: {text}"
}

proc id_ns(argv: List[Str]) [fs, process, env, io, error] {
  let begun = begin("id-ns", ["n:namespace-id=word", ":force", "V:vendor-specific", "b:raw-binary", "H:human-readable"], argv)
  let dev = begun.dev
  let vendor = has(begun.parsed, "vendor-specific")
  if vendor and begun.format == "json" { fail(true, "id-ns: --vendor-specific is not available in json output") }
  let nsid = namespace_of(dev, begun.parsed)
  let selector = if has(begun.parsed, "force") { 17 } else { 0 }
  let data = admin(dev, "identify namespace", OPCODE_IDENTIFY, nsid, selector, 4096).data
  if begun.format == "binary" {
    gnu.write_bytes(data)
    return
  }
  let head = dec.decode(data, dec.NAMESPACE_LAYOUT)
  var fields = head
  if dec.field_value(head, "nsfeat") / 16 % 2 == 1 { fields += dec.decode(data, dec.NAMESPACE_PREFERRED_IO_LAYOUT) }
  fields += dec.decode(data, dec.NAMESPACE_TAIL_LAYOUT)
  var count = dec.field_value(fields, "nlbaf") + dec.field_value(fields, "nulbaf") + 1
  if count > 64 { count = 64 }
  let formats = dec.lba_formats(data, count)
  let current = dec.lba_format_index(dec.field_value(fields, "flbas"))
  if begun.format == "json" {
    var members: List[List[Str]] = [[item.name, field_json(item)] for item in fields]
    let descriptors = [json_object([
      ["ms", json_number(format.metadata_size)],
      ["ds", json_number(format.data_size_shift)],
      ["rp", json_number(format.relative_performance)],
    ]) for format in formats]
    members += [["lbafs", json_array(descriptors)]]
    gnu.write_text(json_object(members) + "\n")
    return
  }
  let human = has(begun.parsed, "human-readable")
  var lines = [f"NVME Identify Namespace {nsid}:"]
  for item in fields {
    lines += [ns_label(item.name, dec.display(item))]
    if human { lines += namespace_detail(item.name, item.value) }
  }
  let performance = ["Best", "Better", "Good", "Degraded"]
  for index in range(formats.len()) {
    let format = formats[index]
    let marker = if index == current { "(in use)" } else { "" }
    let rank = if human { " " + performance[format.relative_performance] } else { "" }
    lines += [f"lbaf {tui.left_pad(f"{index}", 2)} : ms:{tui.right_pad(f"{format.metadata_size}", 3)} lbads:{tui.right_pad(f"{format.data_size_shift}", 2)} rp:{hex(format.relative_performance)}{rank} {marker}"]
  }
  if vendor { lines += ["vs[]:"] + hex_dump(data.slice(384, length: 3712)) }
  gnu.write_text(lines.join("\n") + "\n")
}

proc list_ns(argv: List[Str]) [fs, process, env, io, error] {
  let begun = begin("list-ns", ["n:namespace-id=word", "y:csi=integer", "a:all"], argv, false)
  let dev = begun.dev
  let first = number(begun.parsed, "namespace-id", 1)
  if first == 0 {
    eprint "list-ns: namespace id 0 is invalid"
    exit 1
  }
  let all = has(begun.parsed, "all")
  let specific = has(begun.parsed, "csi")
  let selector = if specific { if all { 27 } else { 26 } } else if all { 16 } else { 2 }
  let csi = number(begun.parsed, "csi", 0)
  if specific and (csi < 0 or csi > 255) {
    eprint f"Expected integer argument for 'csi' but got '{csi}'!"
    exit 1
  }
  let data = admin(dev, "id namespace list", OPCODE_IDENTIFY, first - 1, selector, 4096, cdw11: csi * 16777216).data
  var ids: List[Int] = []
  for index in range(1024) {
    let nsid = dec.le(data, index * 4, 4)
    if nsid == 0 { break }
    ids += [nsid]
  }
  if begun.format == "json" {
    let items = [json_object([["nsid", json_number(nsid)]]) for nsid in ids]
    gnu.write_text(json_object([["nsid_list", json_array(items)]]) + "\n")
    return
  }
  var lines: List[Str] = []
  for index in range(ids.len()) {
    lines += [f"[{tui.left_pad(f"{index}", 4)}]:{hex(ids[index])}"]
  }
  if ! lines.is_empty() { gnu.write_text(lines.join("\n") + "\n") }
}

pure smart_json_key(name: Str) -> Str {
  if name == "endu_grp_crit_warn_sumry" { return "endurance_grp_critical_warning_summary" }
  if name.starts_with("temp_sensor") { return "temperature_sensor_" + name.byte_slice(11) }
  name
}

proc smart_log(argv: List[Str]) [fs, process, env, io, error] {
  let begun = begin("smart-log", ["n:namespace-id=word", "b:raw-binary", "H:human-readable"], argv)
  let dev = begun.dev
  let nsid = number(begun.parsed, "namespace-id", ALL_NAMESPACES)
  let data = get_log(dev, "smart log", LOG_SMART, nsid, dec.SMART_LOG_BYTES)
  if begun.format == "binary" {
    gnu.write_bytes(data)
    return
  }
  let fields = dec.decode(data, dec.SMART_LAYOUT)
  let sensors = [item for item in fields if item.name.starts_with("temp_sensor")]
  if begun.format == "json" {
    var members: List[List[Str]] = []
    for item in fields {
      if item.name.starts_with("temp_sensor") and item.value == 0 { continue }
      members += [[smart_json_key(item.name), item.decimal]]
    }
    gnu.write_text(json_object(members) + "\n")
    return
  }
  let human = has(begun.parsed, "human-readable")
  let warning = dec.field_value(fields, "critical_warning")
  let kelvin = dec.field_value(fields, "temperature")
  print_smart_log(fields, human, warning, kelvin, sensors, nsid, device_name(begun.parsed.operands[0]))
}

pure counter_line(name: Str, fields: List[dec.Field], key: Str) -> Str {
  f"{name}: {dec.group_digits(dec.field(fields, key).decimal)}"
}

proc print_smart_log(fields: List[dec.Field], human: Bool, warning: Int, kelvin: Int, sensors: List[dec.Field], nsid: Int, devname: Str) [process, env, io] {
  var lines = [f"Smart Log for NVME device:{devname} namespace-id:{dec.hex_digits(nsid)}"]
  lines += [f"critical_warning\t\t\t: {hex(warning)}"]
  if human {
    lines += bit_lines(warning, [
      "5 5|Persistent Memory Region has not become read-only|Persistent Memory Region has become read-only",
      "4 4|Volatile memory backup device has not failed|Volatile memory backup device has failed",
      "3 3|Media has not been placed in read only mode|Media has been placed in read only mode",
      "2 2|Reliability has not been degraded|Reliability has been degraded",
      "1 1|Temperature is within acceptable range|Temperature is above the over temperature threshold or below the under temperature threshold",
      "0 0|Available spare space has not fallen below the threshold|Available spare space has fallen below the threshold",
    ], "\t", "\t")
  }
  lines += [f"temperature\t\t\t\t: {temperature_text(kelvin)}"]
  lines += [f"available_spare\t\t\t\t: {dec.field_value(fields, "avail_spare")}%"]
  lines += [f"available_spare_threshold\t\t: {dec.field_value(fields, "spare_thresh")}%"]
  lines += [f"percentage_used\t\t\t\t: {dec.field_value(fields, "percent_used")}%"]
  lines += [f"endurance group critical warning summary: {hex(dec.field_value(fields, "endu_grp_crit_warn_sumry"))}"]
  let read = dec.field(fields, "data_units_read").decimal
  let written = dec.field(fields, "data_units_written").decimal
  lines += [f"Data Units Read\t\t\t\t: {dec.group_digits(read)} ({dec.si_bytes(read, 512000)})"]
  lines += [f"Data Units Written\t\t\t: {dec.group_digits(written)} ({dec.si_bytes(written, 512000)})"]
  lines += [counter_line("host_read_commands\t\t\t", fields, "host_read_commands")]
  lines += [counter_line("host_write_commands\t\t\t", fields, "host_write_commands")]
  lines += [counter_line("controller_busy_time\t\t\t", fields, "controller_busy_time")]
  lines += [counter_line("power_cycles\t\t\t\t", fields, "power_cycles")]
  lines += [counter_line("power_on_hours\t\t\t\t", fields, "power_on_hours")]
  lines += [counter_line("unsafe_shutdowns\t\t\t", fields, "unsafe_shutdowns")]
  lines += [counter_line("media_errors\t\t\t\t", fields, "media_errors")]
  lines += [counter_line("num_err_log_entries\t\t\t", fields, "num_err_log_entries")]
  lines += [f"Warning Temperature Time\t\t: {dec.field_value(fields, "warning_temp_time")}"]
  lines += [f"Critical Composite Temperature Time\t: {dec.field_value(fields, "critical_comp_time")}"]
  for index in range(sensors.len()) {
    if sensors[index].value != 0 {
      lines += [f"Temperature Sensor {index + 1}\t\t\t: {temperature_text(sensors[index].value)}"]
    }
  }
  lines += [f"Thermal Management T1 Trans Count\t: {dec.field_value(fields, "thm_temp1_trans_count")}"]
  lines += [f"Thermal Management T2 Trans Count\t: {dec.field_value(fields, "thm_temp2_trans_count")}"]
  lines += [f"Thermal Management T1 Total Time\t: {dec.field_value(fields, "thm_temp1_total_time")}"]
  lines += [f"Thermal Management T2 Total Time\t: {dec.field_value(fields, "thm_temp2_total_time")}"]
  gnu.write_text(lines.join("\n") + "\n")
}

pure temperature_text(kelvin: Int) -> Str {
  f"{dec.celsius(kelvin)} \u{00b0}C ({kelvin} K)"
}

proc error_log(argv: List[Str]) [fs, process, env, io, error] {
  let begun = begin("error-log", ["e:log-entries=word", "b:raw-binary"], argv)
  let dev = begun.dev
  var entries = number(begun.parsed, "log-entries", 64)
  if entries == 0 {
    eprint "non-zero log-entries is required param"
    exit 1
  }
  let controller = admin(dev, "identify controller", OPCODE_IDENTIFY, 0, 1, 4096).data
  # The controller keeps at most ELPE + 1 entries; asking for more only
  # reads past the end of the page.
  let kept = dec.u8(controller, 262) + 1
  if entries > kept { entries = kept }
  let data = get_log(dev, "error log", LOG_ERROR, ALL_NAMESPACES, entries * dec.ERROR_ENTRY_BYTES)
  if begun.format == "binary" {
    gnu.write_bytes(data)
    return
  }
  let devname = device_name(begun.parsed.operands[0])
  if begun.format == "json" {
    var items: List[Str] = []
    for index in range(entries) {
      let fields = dec.error_entry(data, index)
      let status = dec.field_value(fields, "status_field")
      items += [json_object([
        ["error_count", dec.field(fields, "error_count").decimal],
        ["sqid", json_number(dec.field_value(fields, "sqid"))],
        ["cmdid", json_number(dec.field_value(fields, "cmdid"))],
        ["status_field", json_number(status / 2)],
        ["phase_tag", json_number(status % 2)],
        ["parm_error_location", json_number(dec.field_value(fields, "parm_error_location"))],
        ["lba", dec.field(fields, "lba").decimal],
        ["nsid", json_number(dec.field_value(fields, "nsid"))],
        ["vs", json_number(dec.field_value(fields, "vs"))],
        ["trtype", json_number(dec.field_value(fields, "trtype"))],
        ["cs", dec.field(fields, "cs").decimal],
        ["trtype_spec_info", json_number(dec.field_value(fields, "trtype_spec_info"))],
      ])]
    }
    gnu.write_text(json_object([["errors", json_array(items)]]) + "\n")
    return
  }
  var lines = [f"Error Log Entries for device:{devname} entries:{entries}", "................."]
  for index in range(entries) {
    let fields = dec.error_entry(data, index)
    let status = dec.field_value(fields, "status_field")
    lines += [f" Entry[{tui.left_pad(f"{index}", 2)}]   ", "................."]
    lines += [f"error_count\t: {dec.field(fields, "error_count").decimal}"]
    lines += [f"sqid\t\t: {dec.field_value(fields, "sqid")}"]
    lines += [f"cmdid\t\t: {hex(dec.field_value(fields, "cmdid"))}"]
    lines += [f"status_field\t: {hex(status / 2)}({dec.status_text(status / 2)})"]
    lines += [f"phase_tag\t: {hex(status % 2)}"]
    lines += [f"parm_err_loc\t: {hex(dec.field_value(fields, "parm_error_location"))}"]
    lines += [f"lba\t\t: {hex_field(dec.field(fields, "lba"))}"]
    lines += [f"nsid\t\t: {hex(dec.field_value(fields, "nsid"))}"]
    lines += [f"vs\t\t: {dec.field_value(fields, "vs")}"]
    lines += [f"trtype\t\t: {dec.transport_type_text(dec.field_value(fields, "trtype"))}"]
    lines += [f"cs\t\t: {hex_field(dec.field(fields, "cs"))}"]
    lines += [f"trtype_spec_info: {hex(dec.field_value(fields, "trtype_spec_info"))}"]
    lines += ["................."]
  }
  gnu.write_text(lines.join("\n") + "\n")
}

pure self_test_operation_text(code: Int) -> Str {
  match code {
    0 => "No device self-test operation in progress"
    1 => "Short device self-test operation in progress"
    2 => "Extended device self-test operation in progress"
    14 => "Vendor specific"
    else => "Reserved"
  }
}

proc self_test_log(argv: List[Str]) [fs, process, env, io, error] {
  let begun = begin("self-test-log", ["e:dst-entries=byte"], argv)
  let dev = begun.dev
  var entries = number(begun.parsed, "dst-entries", dec.SELF_TEST_MAX_ENTRIES)
  # Zero asks for the default and the log holds no more than twenty results.
  if entries == 0 or entries > dec.SELF_TEST_MAX_ENTRIES { entries = dec.SELF_TEST_MAX_ENTRIES }
  let length = dec.SELF_TEST_HEADER_BYTES + entries * dec.SELF_TEST_ENTRY_BYTES
  let data = get_log(dev, "self test log", LOG_SELF_TEST, ALL_NAMESPACES, length)
  if begun.format == "binary" {
    gnu.write_bytes(data)
    return
  }
  let operation = dec.u8(data, 0) % 16
  let completion = dec.u8(data, 1) % 128
  let results = dec.self_test_results(data, entries)
  let devname = device_name(begun.parsed.operands[0])
  if begun.format == "json" {
    var items: List[Str] = []
    for item in results {
      var members = [
        ["Self test result", json_number(item.result)],
        ["Self test code", json_number(item.code)],
        ["Segment number", json_number(item.segment)],
        ["Valid Diagnostic Information", json_number(item.valid)],
        ["Power on hours (POH)", item.power_on_hours.decimal],
      ]
      if item.valid % 2 == 1 { members += [["Namespace Identifier", json_number(item.nsid)]] }
      if item.valid / 2 % 2 == 1 { members += [["Failing LBA", item.failing_lba.decimal]] }
      if item.valid / 4 % 2 == 1 { members += [["Status Code Type", json_number(item.status_code_type)]] }
      if item.valid / 8 % 2 == 1 { members += [["Status Code", json_number(item.status_code)]] }
      members += [["Vendor Specific", json_number(item.vendor_specific)]]
      items += [json_object(members)]
    }
    gnu.write_text(json_object([
      ["Current Device Self-Test Operation", json_number(operation)],
      ["Current Device Self-Test Completion", json_number(completion)],
      ["Self Test Results", json_array(items)],
    ]) + "\n")
    return
  }
  var lines = [f"Device Self Test Log for NVME device:{devname}", f"Current operation  : {hex(operation)}"]
  if operation != 0 { lines += [f"Current Completion : {completion}%"] }
  for index in range(results.len()) {
    let item = results[index]
    lines += [f"Self Test Result[{index}]:"]
    lines += [f"  Operation Result             : {hex(item.result)}"]
    lines += [f"  Self Test Code               : {hex(item.code)}"]
    lines += [f"  Valid Diagnostic Information : {hex(item.valid)}"]
    lines += [f"  Power on hours (POH)         : {if item.power_on_hours.hex == "0" { "0" } else { "0x" + item.power_on_hours.hex }}"]
    if item.valid % 2 == 1 { lines += [f"  Namespace Identifier         : {hex(item.nsid)}"] }
    if item.valid / 2 % 2 == 1 { lines += [f"  Failing LBA                  : {if item.failing_lba.hex == "0" { "0" } else { "0x" + item.failing_lba.hex }}"] }
    if item.valid / 4 % 2 == 1 { lines += [f"  Status Code Type             : {hex(item.status_code_type)}"] }
    if item.valid / 8 % 2 == 1 { lines += [f"  Status Code                  : {hex(item.status_code)}"] }
    lines += [f"  Vendor Specific              : {hex(item.vendor_specific % 256)} {hex(item.vendor_specific / 256)}"]
  }
  gnu.write_text(lines.join("\n") + "\n")
}

proc fw_log(argv: List[Str]) [fs, process, env, io, error] {
  let begun = begin("fw-log", ["b:raw-binary"], argv)
  let dev = begun.dev
  let data = get_log(dev, "fw log", LOG_FIRMWARE_SLOT, ALL_NAMESPACES, dec.FIRMWARE_LOG_BYTES)
  if begun.format == "binary" {
    gnu.write_bytes(data)
    return
  }
  let devname = device_name(begun.parsed.operands[0])
  let active = dec.u8(data, 0)
  var slots: List[List[Str]] = []
  for slot in range(1, 8) {
    let magnitude = dec.wide(data, 8 + (slot - 1) * 8, 8)
    if magnitude.hex != "0" { slots += [[f"{slot}", magnitude.hex, dec.firmware_revision(data, slot)]] }
  }
  if begun.format == "json" {
    var members: List[List[Str]] = [["Active Slot", json_number(active % 8)], ["Next Slot", json_number(active / 16 % 8)]]
    for slot in slots { members += [[f"frs{slot[0]}", json_string(slot[2])]] }
    gnu.write_text(json_object([["Firmware Log", json_object(members)]]) + "\n")
    return
  }
  var lines = [f"Firmware Log for device:{devname}", f"afi  : {hex(active)}"]
  for slot in slots {
    let zeros = if slot[1].byte_len() >= 14 { "" } else { "00000000000000".byte_slice(0, length: 14 - slot[1].byte_len()) }
    lines += [f"frs{slot[0]} : 0x{zeros}{slot[1]} ({slot[2]})"]
  }
  gnu.write_text(lines.join("\n") + "\n")
}

# The data buffer length Get Features needs for features whose value is a
# structure rather than the completion dword.
pure feature_data_length(feature_id: Int, cdw11: Int) -> Int {
  match feature_id {
    3 => 4096
    12 => 256
    13 => 4096
    14 => 8
    22 => 512
    129 => if cdw11 % 2 == 1 { 16 } else { 8 }
    else => 0
  }
}

pure power_of_two(exponent: Int) -> Int {
  var out = 1
  for _ in range(exponent) { out = out * 2 }
  out
}

pure feature_detail(feature_id: Int, result: Int) -> List[Str] {
  match feature_id {
    1 => [
      f"\tHigh Priority Weight   (HPW): {result / 16777216 % 256 + 1}",
      f"\tMedium Priority Weight (MPW): {result / 65536 % 256 + 1}",
      f"\tLow Priority Weight    (LPW): {result / 256 % 256 + 1}",
      f"\tArbitration Burst       (AB): {if result % 8 == 7 { "No limit" } else { f"{power_of_two(result % 8)}" }}",
    ]
    2 => [
      f"\tWorkload Hint (WH): {result / 32 % 8}",
      f"\tPower State   (PS): {result % 32}",
    ]
    4 => [
      f"\tThreshold Temperature Select (TMPSEL): {if result / 65536 % 16 == 0 { "Composite temperature" } else if result / 65536 % 16 <= 8 { f"Temperature sensor {result / 65536 % 16}" } else { "Reserved" }}",
      f"\tThreshold Type Select         (THSEL): {if result / 1048576 % 4 == 0 { "Over temperature threshold" } else if result / 1048576 % 4 == 1 { "Under temperature threshold" } else { "Reserved" }}",
      f"\tTemperature Threshold         (TMPTH): {result % 65536} K ({result % 65536 - 273} C)",
    ]
    5 => [
      f"\tDeallocated or Unwritten Logical Block Error Enable (DULBE): {if result / 65536 % 2 == 1 { "Enabled" } else { "Disabled" }}",
      f"\tTime Limited Error Recovery                          (TLER): {result % 65536 * 100} ms",
    ]
    6 => [f"\tVolatile Write Cache Enable (WCE): {if result % 2 == 1 { "Enabled" } else { "Disabled" }}"]
    7 => [
      f"\tNumber of IO Completion Queues Allocated (NCQA): {result / 65536 % 65536 + 1}",
      f"\tNumber of IO Submission Queues Allocated (NSQA): {result % 65536 + 1}",
    ]
    8 => [
      f"\tAggregation Threshold (THR): {result % 256 + 1}",
      f"\tAggregation Time     (TIME): {result / 256 % 256 * 100} usec",
    ]
    10 => [f"\tDisable Normal (DN): {if result % 2 == 1 { "Yes" } else { "No" }}"]
    15 => [f"\tKeep Alive Timeout (KATO): {result} ms"]
    else => []
  }
}

pure select_name(select: Int) -> Str {
  match select {
    0 => "Current"
    1 => "Default"
    2 => "Saved"
    else => "Supported capabilities"
  }
}

proc get_feature(argv: List[Str]) [fs, process, env, io, error] {
  let own = ["f:feature-id=byte", "n:namespace-id=word", "s:sel=byte", "l:data-len=word", "b:raw-binary", "c:cdw11=word", "U:uuid-index=byte", "H:human-readable", "C:changed"]
  let begun = begin("get-feature", own, argv)
  let dev = begun.dev
  if has(begun.parsed, "changed") {
    eprint "get-feature: --changed is not supported"
    exit 1
  }
  if begun.format == "json" {
    eprint "get-feature: json output is not supported"
    exit 1
  }
  let feature_id = number(begun.parsed, "feature-id", 0)
  let select = number(begun.parsed, "sel", 0)
  if select > 3 {
    eprint f"invalid 'select' param:{select}"
    exit 1
  }
  let cdw11 = number(begun.parsed, "cdw11", 0)
  var length = feature_data_length(feature_id, cdw11)
  if has(begun.parsed, "data-len") { length = number(begun.parsed, "data-len", 0) }
  let nsid = number(begun.parsed, "namespace-id", 0)
  let reply = admin(dev, "get-feature", OPCODE_GET_FEATURES, nsid, select * 256 + feature_id, length, cdw11: cdw11, cdw14: number(begun.parsed, "uuid-index", 0))
  if begun.format == "binary" and length > 0 {
    gnu.write_bytes(reply.data)
    return
  }
  var lines = [f"get-feature:{hex_padded(feature_id, 2)} ({dec.feature_name(feature_id)}), {select_name(select)} value:{hex_padded(reply.result, 8)}"]
  if select == 3 {
    lines += [if reply.result % 2 == 1 { "\tfeature is saveable" } else { "\tfeature is not saveable" }]
    lines += [if reply.result / 2 % 2 == 1 { "\tfeature is namespace specific" } else { "\tfeature is not namespace specific" }]
    lines += [if reply.result / 4 % 2 == 1 { "\tfeature is changeable" } else { "\tfeature is not changeable" }]
  } else if has(begun.parsed, "human-readable") {
    lines += feature_detail(feature_id, reply.result)
  }
  if length > 0 { lines += hex_dump(reply.data) }
  gnu.write_text(lines.join("\n") + "\n")
}

proc set_feature(argv: List[Str]) [fs, process, env, io, error] {
  let own = ["n:namespace-id=word", "f:feature-id=byte", "V:value=word", "c:cdw12=word", "U:uuid-index=byte", "l:data-len=word", "d:data=string", "s:save"]
  let parsed = parse("set-feature", own, argv)
  for long in ["cdw12", "uuid-index", "data-len", "data"] {
    if has(parsed, long) {
      eprint f"set-feature: --{long} is not supported: only features with a one-dword value can be set"
      exit 1
    }
  }
  let fd = open_device("set-feature", parsed)
  let format = output_format(parsed, true)
  if format != "normal" {
    eprint "set-feature: only the normal output format is available"
    exit 1
  }
  if ! has(parsed, "feature-id") {
    eprint "feature-id required param"
    exit 1
  }
  if ! has(parsed, "value") {
    eprint "value required param"
    exit 1
  }
  let feature_id = number(parsed, "feature-id", 0)
  if feature_id not in SETTABLE_FEATURES {
    let allowed = [hex_padded(id, 2) for id in SETTABLE_FEATURES].join(", ")
    eprint f"set-feature: feature {hex_padded(feature_id, 2)} cannot be changed with this nvme; settable features: {allowed}"
    exit 1
  }
  let dev = make_session("set-feature", fd, format, parsed)
  let value = number(parsed, "value", 0)
  let save = if has(parsed, "save") { 2147483648 } else { 0 }
  let nsid = number(parsed, "namespace-id", 0)
  let _ = admin(dev, "set-feature", 9, nsid, save + feature_id, 0, cdw11: value, change: true)
  gnu.write_text(f"set-feature:{hex_padded(feature_id, 2)} ({dec.feature_name(feature_id)}), value:{hex_padded(value, 8)}\n")
}

proc device_self_test(argv: List[Str]) [fs, process, env, io, error] {
  let begun = begin("device-self-test", ["n:namespace-id=word", "s:self-test-code=byte", "w:wait"], argv)
  let dev = begun.dev
  if has(begun.parsed, "wait") {
    eprint "device-self-test: --wait is not supported; poll self-test-log instead"
    exit 1
  }
  let code = number(begun.parsed, "self-test-code", 0)
  if code != 0 and code != 1 and code != 2 and code != 15 {
    eprint f"device-self-test: self-test code {hex(code)} is not supported (0 shows the state, 1 short, 2 extended, 15 abort)"
    exit 1
  }
  let nsid = number(begun.parsed, "namespace-id", ALL_NAMESPACES)
  if code == 0 {
    let data = get_log(dev, "self test log", LOG_SELF_TEST, ALL_NAMESPACES, dec.SELF_TEST_HEADER_BYTES)
    let operation = dec.u8(data, 0) % 16
    var lines = [f"Current operation  : {hex(operation)} ({self_test_operation_text(operation)})"]
    if operation != 0 { lines += [f"Current Completion : {dec.u8(data, 1) % 128}%"] }
    gnu.write_text(lines.join("\n") + "\n")
    return
  }
  let _ = admin(dev, "device self-test", OPCODE_DEVICE_SELF_TEST, nsid, code, 0, change: true)
  let text = if code == 15 { "Aborting device self-test operation" } else if code == 2 { "Extended Device self-test started" } else { "Short Device self-test started" }
  gnu.write_text(text + "\n")
}

# One namespace row of `nvme list`.
type ListedNamespace = {
  name: Str, generic: Str, serial: Str, model: Str, firmware: Str, nsid: Int,
  block_size: Int, block_count: Int, used_blocks: Int, metadata_size: Int,
}

# A number kept in a sysfs attribute file, or null when it is absent.
proc sysfs_number(file: Path) [fs] -> Int? {
  if let Ok(text) = file.read_text() {
    if let Ok(value) = text.trim().parse_int() { value } else { null }
  } else {
    null
  }
}

# Reads what the namespace node itself reports (Identify Namespace) when it
# can be opened; nvme-cli falls back to sysfs the same way, because an
# unprivileged user cannot open the node. Returns [block size, blocks, used
# blocks, metadata size] or null.
proc identify_listed(node: Path, nsid: Int) [fs, process, error] -> List[Int]? {
  match unix.open_fd(node) {
    Err(_) => null
    Ok(fd) => {
      let outcome = linux.nvme_admin(fd, OPCODE_IDENTIFY, 4096, nsid: nsid, cdw10: 0)
      let _ = unix.close_fd(fd)
      match outcome {
        Err(_) => null
        Ok(reply) => {
          if reply.status != 0 or reply.data.len() != 4096 { return null }
          let fields = dec.decode(reply.data, dec.NAMESPACE_LAYOUT)
          let formats = dec.lba_formats(reply.data, dec.field_value(fields, "nlbaf") + 1)
          let index = dec.lba_format_index(dec.field_value(fields, "flbas"))
          if index >= formats.len() or formats[index].data_size_shift > 30 { return null }
          var size = 1
          for _ in range(formats[index].data_size_shift) { size = size * 2 }
          [size, dec.field_value(fields, "nsze"), dec.field_value(fields, "nuse"), formats[index].metadata_size]
        }
      }
    }
  }
}

proc listed_namespaces(sys_root: Path, dev_root: Path) [fs, process, env, io, error] -> List[ListedNamespace] {
  let found = match linux.storage_candidates(sys_root: sys_root, dev_root: dev_root) {
    Ok(items) => items
    Err(failure) => {
      fail(false, f"Failed to scan nvme subsystems: {gnu.strerror(failure)}")
      []
    }
  }
  var rows: List[ListedNamespace] = []
  for item in found {
    if item.kind != "nvme_namespace" { continue }
    let base = fp"{sys_root}/block/{item.name}"
    let pieces = item.name.split("n")
    let nsid = sysfs_number(fp"{base}/nsid") ?? pieces[pieces.len() - 1].parse_int() ?? 0
    var block_size = sysfs_number(fp"{base}/queue/logical_block_size") ?? 512
    if block_size <= 0 { block_size = 512 }
    var block_count = (item.size_bytes ?? 0) / block_size
    var used = block_count
    var metadata = sysfs_number(fp"{base}/metadata_bytes") ?? 0
    let measured = identify_listed(item.path, nsid) ?? []
    if measured.len() == 4 {
      block_size = measured[0]
      block_count = measured[1]
      used = measured[2]
      metadata = measured[3]
    }
    rows += [{
      name: item.name, generic: "ng" + item.name.byte_slice(4), serial: item.serial ?? "",
      model: item.model ?? "", firmware: item.firmware ?? "", nsid: nsid,
      block_size: block_size, block_count: block_count, used_blocks: used, metadata_size: metadata,
    }]
  }
  rows
}

# A byte count in SI prefixes the way nvme-cli columns show it, `%6.2f %2sB`.
pure si_column(bytes_count: Int) -> Str {
  var amount = bytes_count.float()
  let prefixes = ["", "k", "M", "G", "T", "P", "E", "Z", "Y"]
  var index = 0
  while amount >= 1000.0 and index < prefixes.len() - 1 {
    amount = amount / 1000.0
    index += 1
  }
  tui.left_pad(amount.format_number("f", 2) ?? "0.00", 6) + " " + tui.left_pad(prefixes[index], 2) + "B"
}

# A block size in binary prefixes, `%3.0f %2sB`.
pure binary_column(block_size: Int) -> Str {
  var amount = block_size
  let prefixes = ["", "Ki", "Mi", "Gi", "Ti"]
  var index = 0
  while amount >= 1024 and index < prefixes.len() - 1 {
    amount = amount / 1024
    index += 1
  }
  tui.left_pad(f"{amount}", 3) + " " + tui.left_pad(prefixes[index], 2) + "B"
}

proc list_devices(argv: List[Str], sys_root: Path, dev_root: Path) [fs, process, env, io, error] {
  let parsed = parse("list", [], argv)
  if ! parsed.operands.is_empty() { usage_failure("list", f"unexpected extra argument: {parsed.operands[0]}") }
  let format = output_format(parsed, false)
  if has(parsed, "verbose") {
    eprint "list: verbose listing is not supported"
    exit 1
  }
  if has(parsed, "dry-run") { exit 0 }
  let rows = listed_namespaces(sys_root, dev_root)
  if format == "json" {
    var items: List[Str] = []
    for row in rows {
      items += [json_object([
        ["NameSpace", json_number(row.nsid)],
        ["DevicePath", json_string(row.name)],
        ["GenericPath", json_string(row.generic)],
        ["Firmware", json_string(row.firmware)],
        ["ModelNumber", json_string(row.model)],
        ["SerialNumber", json_string(row.serial)],
        ["UsedBytes", json_number(row.used_blocks * row.block_size)],
        ["MaximumLBA", json_number(row.block_count)],
        ["PhysicalSize", json_number(row.block_count * row.block_size)],
        ["SectorSize", json_number(row.block_size)],
      ])]
    }
    gnu.write_text(json_object([["Devices", json_array(items)]]) + "\n")
    return
  }
  if rows.is_empty() {
    eprint "No NVMe devices detected."
    return
  }
  var lines: List[Str] = []
  let widths = [21, 21, 20, 40, 10, 26, 16, 8]
  let titles = ["Node", "Generic", "SN", "Model", "Namespace", "Usage", "Format", "FW Rev"]
  lines += [[tui.right_pad(titles[index], widths[index]) for index in range(8)].join(" ")]
  lines += [[tui.right_pad("", widths[index]).replace(" ", with: "-") for index in range(8)].join(" ")]
  for row in rows {
    let usage = f"{si_column(row.used_blocks * row.block_size)} / {si_column(row.block_count * row.block_size)}"
    let format_text = f"{binary_column(row.block_size)} + {tui.left_pad(f"{row.metadata_size}", 2)} B"
    let cells = [row.name, row.generic, row.serial, row.model, hex(row.nsid), usage, format_text, row.firmware]
    lines += [[tui.right_pad(cells[index], widths[index]) for index in range(8)].join(" ")]
  }
  gnu.write_text(lines.join("\n") + "\n")
}

## Runs `nvme ARGV`. `sys_root` is where `list` finds NVMe namespaces and
## `dev_root` is the directory their device nodes are named under.
export proc dispatch(argv: List[Str], sys_root: Path, dev_root: Path) [fs, process, env, io, error] {
  if argv.is_empty() {
    gnu.write_text(main_usage() + "\n")
    return
  }
  let cmd = argv[0]
  let rest = argv[1..]
  match cmd {
    "version" => gnu.write_text(VERSION_TEXT + "\n")
    "--version" => gnu.write_text(VERSION_TEXT + "\n")
    "help" => help_command(rest)
    "-h" => help_command(rest)
    "--help" => help_command(rest)
    "list" => list_devices(rest, sys_root, dev_root)
    "id-ctrl" => id_ctrl(rest)
    "id-ns" => id_ns(rest)
    "list-ns" => list_ns(rest)
    "smart-log" => smart_log(rest)
    "error-log" => error_log(rest)
    "self-test-log" => self_test_log(rest)
    "fw-log" => fw_log(rest)
    "get-feature" => get_feature(rest)
    "set-feature" => set_feature(rest)
    "device-self-test" => device_self_test(rest)
    else => unavailable(cmd)
  }
}

proc unavailable(cmd: Str) [process, env, io] -> Unit {
  if cmd in MUTATING_COMMANDS {
    eprint f"nvme: {cmd} is refused: this nvme never formats, rewrites firmware, or otherwise changes a device beyond set-feature and device-self-test"
    exit 1
  }
  if cmd in KNOWN_COMMANDS.split(" ") {
    eprint f"nvme: {cmd} is not supported"
    exit 1
  }
  eprint f"ERROR: Invalid sub-command '{cmd}'"
  eprint main_usage()
  exit 1
}

proc help_command(rest: List[Str]) [process, env, io] {
  if rest.is_empty() or usage_text(rest[0]) == "" {
    gnu.write_text(main_usage() + "\n")
    return
  }
  gnu.write_text(usage_with_headings(rest[0]) + "\n")
}
