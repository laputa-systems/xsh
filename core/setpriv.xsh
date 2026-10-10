#!/bin/xsh
##! setpriv: run a program with different privilege settings.
##!
##! Options are parsed in command-line order, like util-linux getopt_long with
##! a leading `+`: the first operand starts the command. Every request is
##! validated before any privilege changes, then applied in an order that keeps
##! the privileges later steps need: bounding set, securebits, no_new_privs,
##! supplementary groups, GIDs, UIDs, inheritable and ambient capabilities, and
##! last the parent-death signal and ptracer, because the kernel clears the
##! parent-death signal whenever credentials change.
use lib.capability
use lib.gnu

## One edit of a capability set: add or remove one capability, or all of them.
type Edit = {add: Bool, all: Bool, cap: Int}
## One edit of the securebit flags; `name` is a flag name or `all`.
type FlagEdit = {add: Bool, name: Str}

## The shape of `linux.privileges()`.
type Privileges = {
  effective: List[Int], permitted: List[Int], inheritable: List[Int],
  bounding: List[Int], ambient: List[Int], securebits: List[Str],
  no_new_privs: Bool, parent_death_signal: Int, last_capability: Int,
}
## One row of the platform signal table.
type SignalEntry = {name: Str, number: Int}
## The account fields --reset-env reads.
type Account = {gid: Int, home: Path, name: Str, shell: Str, uid: Int}

## What the command line asked for. Null fields were not given.
type Request = {
  dump: Int, list_caps: Int, no_new_privs: Bool, reset_env: Bool,
  ruid: Int?, euid: Int?, rgid: Int?, egid: Int?,
  group_mode: Str?, groups: List[Int], init_user: Int?,
  inheritable: List[Edit]?, ambient: List[Edit]?, bounding: List[Edit]?,
  securebits: List[FlagEdit]?,
  pdeathsig: Str?, ptracer: Int?,
  others: Bool, command: List[Str],
}

# Declared alphabetically so an ambiguous abbreviation lists its candidates in
# the order GNU getopt_long does.
const OPTIONS = ["ambient-caps", "apparmor-profile", "bounding-set", "clear-groups", "dump", "egid", "euid", "groups", "help", "inh-caps", "init-groups", "keep-groups", "landlock-access", "landlock-rule", "list-caps", "nnp", "no-new-privs", "pdeathsig", "ptracer", "regid", "reset-env", "reuid", "rgid", "ruid", "seccomp-filter", "securebits", "selinux-label", "version"]
const VALUED = ["ambient-caps", "apparmor-profile", "bounding-set", "egid", "euid", "groups", "inh-caps", "landlock-access", "landlock-rule", "pdeathsig", "ptracer", "regid", "reuid", "rgid", "ruid", "securebits", "seccomp-filter", "selinux-label"]
# Security modules and filters the platform layer has no primitive for. They
# are recognized so they fail by name instead of being mistaken for typos.
const UNAVAILABLE = ["apparmor-profile", "landlock-access", "landlock-rule", "seccomp-filter", "selinux-label"]
const SECUREBIT_NAMES = ["noroot", "noroot_locked", "no_setuid_fixup", "no_setuid_fixup_locked", "keep_caps_locked"]
const ROOT_PATH = "/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin"
const USER_PATH = "/usr/local/bin:/bin:/usr/bin"

const HELP = """Usage:
 setpriv [options] <program> [<argument>...]

Run a program with different privilege settings.

Options:
 -d, --dump                  show current state (and do not exec)
 --nnp, --no-new-privs       disallow granting new privileges
 --ambient-caps <caps>       set ambient capabilities
 --inh-caps <caps>           set inheritable capabilities
 --bounding-set <caps>       set capability bounding set
 --ruid <uid|user>           set real uid
 --euid <uid|user>           set effective uid
 --rgid <gid|group>          set real gid
 --egid <gid|group>          set effective gid
 --reuid <uid|user>          set real and effective uid
 --regid <gid|group>         set real and effective gid
 --clear-groups              clear supplementary groups
 --keep-groups               keep supplementary groups
 --init-groups               initialize supplementary groups
 --list-caps                 list all known capabilities
 --groups <group>[,...]      set supplementary group(s) by GID or name
 --securebits <bits>         set securebits
 --pdeathsig keep|clear|<signame>
                             set or clear parent death signal
 --ptracer <pid>|any|none    allow ptracing from the given process
 --reset-env                 clear all environment and initialize
                               HOME, SHELL, USER, LOGNAME and PATH

 -h, --help                  display this help
 -V, --version               display version

This tool can be dangerous.  Read the manpage, and be careful.
For more details see setpriv(1).
"""

# A failed privilege change cannot be rolled back, so the applet ends at once
# with the kernel's wording and the status util-linux uses for them.
proc die(what: Str, reason: Str) [process, env, error] {
  gnu.error(f"{what}: {reason}")
  exit 127
}

proc die_with(what: Str, failure: Error) [process, env, error] {
  die(what, gnu.strerror(failure))
}

proc usage_fail(message: Str) [process, env, error] {
  gnu.error(message)
  exit 1
}

proc read_state() [process, env, error] -> Privileges {
  match linux.privileges() {
    Ok(state) => state
    Err(failure) => {
      die_with("read privileges", failure)
      exit 127
    }
  }
}

pure base_name(entry: capability.CapabilityName) -> Str { entry.name.byte_slice(4) }

# Capability numbers keep their names up to the highest one this program
# knows; later ones print as cap_N, which the parser also accepts.
pure cap_name(number: Int) -> Str {
  for entry in capability.CAPABILITIES {
    if entry.bit == number { return base_name(entry) }
  }
  f"cap_{number}"
}

pure cap_list(numbers: List[Int]) -> Str {
  return "[none]" when numbers.is_empty()
  collect { for number in numbers { yield cap_name(number) } }.join(",")
}

pure cap_number(name: Str, last: Int) -> Int? {
  let lower = name.lower()
  for entry in capability.CAPABILITIES {
    if base_name(entry) == lower { return entry.bit }
  }
  if rx"^cap_[0-9]+$".matches(lower) {
    let number = lower.byte_slice(4).parse_int() ?? -1
    return number when number >= 0 and number <= last
  }
  null
}

# Items are comma separated and each carries a `+` or `-`, as in util-linux.
proc parse_caps(text: Str) [process, env, error] -> List[Edit] {
  let last = read_state().last_capability
  var edits: List[Edit] = []
  for item in text.split(",") {
    if item == "" or ! (item.starts_with("+") or item.starts_with("-")) {
      usage_fail("bad capability string")
    }
    let add = item.starts_with("+")
    let name = item.byte_slice(1)
    if name == "all" {
      edits += [{add: add, all: true, cap: 0}]
    } else if let number = cap_number(name, last) {
      edits += [{add: add, all: false, cap: number}]
    } else {
      usage_fail(f"unknown capability \"{name}\"")
    }
  }
  edits
}

proc parse_securebits(text: Str) [process, env, error] -> List[FlagEdit] {
  var edits: List[FlagEdit] = []
  for item in text.split(",") {
    if item == "" or ! (item.starts_with("+") or item.starts_with("-")) {
      usage_fail("bad securebits string")
    }
    let add = item.starts_with("+")
    let name = item.byte_slice(1)
    if name == "all" {
      if add { usage_fail("+all securebits is not allowed") }
    } else if name in ["keep_caps"] {
      usage_fail("adjusting keep_caps does not make sense")
    } else if ! (name in SECUREBIT_NAMES) {
      usage_fail("unrecognized securebit")
    }
    edits += [{add: add, name: name}]
  }
  edits
}

# IDs are decimal or an account name; the all-ones value is the kernel's
# "unchanged" sentinel and never an identity.
proc parse_user(kind: Str, text: Str) [process, env, fs, error] -> Int {
  if rx"^[0-9]+$".matches(text) {
    let number = text.parse_int() ?? -1
    if number >= 0 and number < 4294967295 { return number }
  } else if text != "" {
    if let Ok(account) = user.lookup(text) { return account.uid }
  }
  usage_fail(f"failed to parse {kind}: '{text}'")
  exit 1
}

proc parse_group(kind: Str, text: Str) [process, env, fs, error] -> Int {
  if rx"^[0-9]+$".matches(text) {
    let number = text.parse_int() ?? -1
    if number >= 0 and number < 4294967295 { return number }
  } else if text != "" {
    if let Ok(entry) = group.lookup(text) { return entry.gid }
  }
  usage_fail(f"failed to parse {kind}: '{text}'")
  exit 1
}

proc parse_group_list(text: Str) [process, env, fs, error] -> List[Int] {
  var ids: List[Int] = []
  for item in text.split(",") {
    var found: Int? = null
    if rx"^[0-9]+$".matches(item) {
      let number = item.parse_int() ?? -1
      if number >= 0 and number < 4294967295 { found = number }
    } else if item != "" {
      if let Ok(entry) = group.lookup(item) { found = entry.gid }
    }
    if let id = found { ids += [id] } else { usage_fail(f"Invalid supplementary group id: '{item}'") }
  }
  ids
}

# Signal numbers come from the platform table, so real-time signals follow
# the C library's range. RTMIN+n and RTMAX-n select within it.
proc signal_number(name: Str) [process, env, error] -> Int {
  var text = name.upper()
  if text.starts_with("SIG") { text = text.byte_slice(3) }
  return 6 when text == "IOT"
  return 29 when text == "POLL"
  return 31 when text == "UNUSED"
  let table = process.signals()
  var low = 0
  var high = 0
  for entry in table {
    if entry.name == "RTMIN" { low = entry.number }
    if entry.name == "RTMAX" { high = entry.number }
    if entry.name == text and entry.number > 0 and text != "RTMIN" and text != "RTMAX" { return entry.number }
  }
  if rx"^RTMIN\+[0-9]+$".matches(text) {
    let offset = text.byte_slice(6).parse_int() ?? 1000
    if low + offset <= high { return low + offset }
  }
  if rx"^RTMAX-[0-9]+$".matches(text) {
    let offset = text.byte_slice(6).parse_int() ?? 1000
    if high - offset >= low { return high - offset }
  }
  usage_fail(f"unknown signal: {name}")
  exit 1
}

pure signal_label(number: Int, table: List[SignalEntry]) -> Str {
  return "[none]" when number == 0
  return "UNUSED" when number == 31
  for entry in table {
    if entry.number == number and number < 32 and number > 0 { return entry.name }
  }
  f"{number}"
}

pure canonical(option: Str) -> Str {
  return "nnp" when option == "no-new-privs"
  option
}

# GNU getopt_long matching: an exact name wins, otherwise a unique prefix;
# aliases of one option never make a prefix ambiguous.
proc resolve_option(name: Str) [process, env, error] -> Str {
  let known = OPTIONS
  if name in known { return canonical(name) }
  let matches = collect { for option in known { yield option when option.starts_with(name) } }
  let distinct = collect { for option in matches { yield canonical(option) } }.to_set().to_list()
  if distinct.len() == 1 { return distinct[0] }
  if matches.len() > 1 {
    let listed = collect { for option in matches { yield f"'--{option}'" } }.join(" ")
    gnu.usage_error(f"option '--{name}' is ambiguous; possibilities: {listed}")
  }
  gnu.usage_error(f"unrecognized option '--{name}'")
  exit 1
}

proc parse(argv: List[Str]) [process, env, fs, error, io] -> Request {
  var dumps = 0
  var list_caps = 0
  var no_new_privs = false
  var reset_env = false
  var ruid: Int? = null
  var euid: Int? = null
  var rgid: Int? = null
  var egid: Int? = null
  var group_mode: Str? = null
  var group_mode_name = ""
  var groups: List[Int] = []
  var inheritable: List[Edit]? = null
  var ambient: List[Edit]? = null
  var bounding: List[Edit]? = null
  var securebits: List[FlagEdit]? = null
  var pdeathsig: Str? = null
  var ptracer: Int? = null
  var others = false
  var index = 0
  while index < argv.len() {
    let word = argv[index]
    if word == "--" {
      index += 1
      break
    }
    if ! word.starts_with("-") or word == "-" { break }
    index += 1
    var names: List[Str] = []
    var attached: Str? = null
    if word.starts_with("--") {
      let body = word.byte_slice(2)
      let equal = body.find("=")
      let given = if let at = equal { body.byte_slice(0, at) } else { body }
      let option = resolve_option(given)
      if let at = equal { attached = body.byte_slice(at + 1) }
      names = [option]
    } else {
      for letter in word.byte_slice(1).split("") {
        if letter == "d" { names += ["dump"] } else if letter == "h" { names += ["help"] } else if letter == "V" { names += ["version"] } else {
          gnu.usage_error(f"invalid option -- '{letter}'")
        }
      }
    }
    for option in names {
      var value = ""
      let valued = option in VALUED
      if valued {
        if let text = attached {
          value = text
        } else if index < argv.len() {
          value = argv[index]
          index += 1
        } else {
          gnu.usage_error(f"option '--{option}' requires an argument")
        }
      } else if attached != null {
        gnu.usage_error(f"option '--{option}' doesn't allow an argument")
      }
      if option == "help" {
        gnu.help("\n" + HELP)
        exit 0
      }
      if option == "version" {
        gnu.version("setpriv")
        exit 0
      }
      if option in UNAVAILABLE {
        usage_fail(f"option '--{option}' is not supported by this implementation")
      }
      if option == "dump" {
        dumps += 1
        continue
      }
      if option == "list-caps" {
        list_caps += 1
        continue
      }
      others = true
      if option == "nnp" {
        if no_new_privs { usage_fail("duplicate --no-new-privs option") }
        no_new_privs = true
      } else if option == "reset-env" {
        reset_env = true
      } else if option == "ruid" {
        if ruid != null { usage_fail("duplicate ruid") }
        ruid = parse_user("ruid", value)
      } else if option == "euid" {
        if euid != null { usage_fail("duplicate euid") }
        euid = parse_user("euid", value)
      } else if option == "reuid" {
        if ruid != null or euid != null { usage_fail("duplicate ruid or euid") }
        let id = parse_user("reuid", value)
        ruid = id
        euid = id
      } else if option == "rgid" {
        if rgid != null { usage_fail("duplicate rgid") }
        rgid = parse_group("rgid", value)
      } else if option == "egid" {
        if egid != null { usage_fail("duplicate egid") }
        egid = parse_group("egid", value)
      } else if option == "regid" {
        if rgid != null or egid != null { usage_fail("duplicate rgid or egid") }
        let id = parse_group("regid", value)
        rgid = id
        egid = id
      } else if option in ["keep-groups", "clear-groups", "init-groups", "groups"] {
        if group_mode_name == option { usage_fail(f"duplicate --{option} option") }
        if group_mode != null { usage_fail(f"options --{group_mode_name} and --{option} cannot be combined") }
        group_mode = if option == "groups" { "list" } else { option.byte_slice(0, option.find("-") ?? 0) }
        group_mode_name = option
        if option == "groups" { groups = parse_group_list(value) }
      } else if option == "inh-caps" {
        if inheritable != null { usage_fail("duplicate --inh-caps option") }
        inheritable = parse_caps(value)
      } else if option == "ambient-caps" {
        if ambient != null { usage_fail("duplicate --ambient-caps option") }
        ambient = parse_caps(value)
      } else if option == "bounding-set" {
        if bounding != null { usage_fail("duplicate --bounding-set option") }
        bounding = parse_caps(value)
      } else if option == "securebits" {
        if securebits != null { usage_fail("duplicate --securebits option") }
        securebits = parse_securebits(value)
      } else if option == "pdeathsig" {
        if pdeathsig != null { usage_fail("duplicate --pdeathsig option") }
        pdeathsig = value
        if value not in ["keep", "clear"] {
          let _ = signal_number(value)
        }
      } else if option == "ptracer" {
        if ptracer != null { usage_fail("duplicate --ptracer option") }
        if value == "any" { ptracer = -1 } else if value == "none" { ptracer = 0 } else {
          let pid = if rx"^[0-9]+$".matches(value) { value.parse_int() ?? 0 } else { 0 }
          if pid <= 0 or pid > 2147483647 { usage_fail(f"invalid PID argument: '{value}'") }
          ptracer = pid
        }
      }
    }
  }
  let command = if index < argv.len() { argv[index..] } else { [] }
  if dumps > 0 {
    if others or list_caps > 0 or ! command.is_empty() { usage_fail("--dump is incompatible with all other options") }
  } else if list_caps > 0 {
    if others or list_caps > 1 or ! command.is_empty() { usage_fail("--list-caps must be specified alone") }
  } else if command.is_empty() {
    usage_fail("No program specified")
  }
  if (rgid != null or egid != null) and group_mode == null {
    usage_fail("--[re]gid requires --keep-groups, --clear-groups, --init-groups, or --groups")
  }
  var init_user: Int? = null
  if group_mode == "init" {
    if ruid == null { usage_fail("--init-groups requires --ruid or --reuid") }
    init_user = ruid
  }
  {
    dump: dumps, list_caps: list_caps, no_new_privs: no_new_privs, reset_env: reset_env,
    ruid: ruid, euid: euid, rgid: rgid, egid: egid,
    group_mode: group_mode, groups: groups, init_user: init_user,
    inheritable: inheritable, ambient: ambient, bounding: bounding, securebits: securebits,
    pdeathsig: pdeathsig, ptracer: ptracer, others: others, command: command,
  }
}

pure edit_caps(current: List[Int], edits: List[Edit], last: Int) -> List[Int] {
  var members = current
  for edit in edits {
    if edit.all {
      members = if edit.add { collect { for number in range(last + 1) { yield number } } } else { [] }
    } else if edit.add {
      if ! (edit.cap in members) { members += [edit.cap] }
    } else {
      members = collect { for number in members { yield number when number != edit.cap } }
    }
  }
  members
}

# Capabilities the command line named with `+name`: failing to grant one is an
# error, while `+all` only covers whatever the kernel allows.
pure named_additions(edits: List[Edit]) -> List[Int] {
  collect { for edit in edits { yield edit.cap when edit.add and ! edit.all } }
}

proc apply_bounding(edits: List[Edit]) [process, env, error] {
  let state = read_state()
  let target = edit_caps(state.bounding, edits, state.last_capability)
  for cap in named_additions(edits) {
    if cap in target and ! (cap in state.bounding) { die("apply bounding set", "Operation not permitted") }
  }
  for cap in state.bounding {
    if ! (cap in target) {
      if let Err(failure) = linux.drop_bounding_capability(cap) { die_with("apply bounding set", failure) }
    }
  }
}

proc apply_inheritable(edits: List[Edit]) [process, env, error] {
  let state = read_state()
  let target = edit_caps(state.inheritable, edits, state.last_capability)
  let applied = linux.set_capabilities(effective: state.effective, permitted: state.permitted, inheritable: target)
  if let Err(failure) = applied { die_with("apply capabilities", failure) }
}

proc apply_ambient(edits: List[Edit]) [process, env, error] {
  let state = read_state()
  let target = edit_caps(state.ambient, edits, state.last_capability)
  for cap in state.ambient {
    if ! (cap in target) {
      if let Err(failure) = linux.set_ambient_capability(cap, false) { die_with("apply ambient capabilities", failure) }
    }
  }
  let named = named_additions(edits)
  for cap in target {
    if ! (cap in state.ambient) {
      if let Err(failure) = linux.set_ambient_capability(cap, true) {
        if cap in named { die_with("apply ambient capabilities", failure) }
      }
    }
  }
}

proc apply_securebits(edits: List[FlagEdit]) [process, env, error] {
  var flags = read_state().securebits
  for edit in edits {
    if edit.name == "all" {
      flags = []
    } else if edit.add {
      if ! (edit.name in flags) { flags += [edit.name] }
    } else {
      flags = collect { for flag in flags { yield flag when flag != edit.name } }
    }
  }
  if let Err(failure) = linux.set_securebits(flags) { die_with("set process securebits failed", failure) }
}

# The user whose account supplies --init-groups must exist before anything
# changes, so a missing account leaves the process untouched.
proc init_group_ids(uid: Int) [process, env, fs, error] -> List[Int] {
  match user.by_uid(uid) {
    Ok(account) => {
      match user.groups(account.name, primary_gid: account.gid) {
        Ok(ids) => ids
        Err(failure) => {
          die_with("initgroups failed", failure)
          exit 127
        }
      }
    }
    Err(_) => {
      usage_fail(f"uid {uid} not found, --init-groups requires an user that can be found on the system")
      exit 1
    }
  }
}

proc apply_groups(request: Request, init_ids: List[Int]) [process, env, error] {
  let mode = request.group_mode
  if mode == "clear" {
    if let Err(failure) = unix.set_groups([]) { die_with("setgroups failed", failure) }
  } else if mode == "init" {
    if let Err(failure) = unix.set_groups(init_ids) { die_with("setgroups failed", failure) }
  } else if mode == "list" {
    if let Err(failure) = unix.set_groups(request.groups) { die_with("setgroups failed", failure) }
  }
}

# The saved ID follows the effective ID when one is given and is otherwise
# left alone, so a lone --ruid keeps the old effective and saved IDs.
proc apply_uids(request: Request) [process, env, error] {
  if request.ruid == null and request.euid == null { return }
  let applied = unix.set_resuid(real: request.ruid, effective: request.euid, saved: request.euid)
  if let Err(failure) = applied { die_with("setresuid failed", failure) }
}

proc apply_gids(request: Request) [process, env, error] {
  if request.rgid == null and request.egid == null { return }
  let applied = unix.set_resgid(real: request.rgid, effective: request.egid, saved: request.egid)
  if let Err(failure) = applied { die_with("setresgid failed", failure) }
}

proc dump(level: Int) [fs, process, env, io, error] {
  let state = read_state()
  let identity = unix.id()
  if let Err(failure) = identity { die_with("read credentials", failure) }
  let ids = identity?
  let listed = ids.supplementary
  var lines = [
    f"uid: {ids.uid}",
    f"euid: {ids.euid}",
    f"gid: {ids.gid}",
    f"egid: {ids.egid}",
    f"Supplementary groups: {if listed.is_empty() { "[none]" } else { collect { for id in listed { yield f"{id}" } }.join(",") }}",
    f"no_new_privs: {if state.no_new_privs { 1 } else { 0 }}",
  ]
  if level >= 2 {
    lines += [f"Effective capabilities: {cap_list(state.effective)}", f"Permitted capabilities: {cap_list(state.permitted)}"]
  }
  let flags = if state.securebits.is_empty() { "[none]" } else { state.securebits.join(",") }
  lines += [
    f"Inheritable capabilities: {cap_list(state.inheritable)}",
    f"Ambient capabilities: {cap_list(state.ambient)}",
    f"Capability bounding set: {cap_list(state.bounding)}",
    f"Securebits: {flags}",
    f"Parent death signal: {signal_label(state.parent_death_signal, process.signals())}",
  ]
  gnu.write_text(lines.join("\n") + "\n")
}

# util-linux --reset-env: keep TERM, then HOME, SHELL, USER, LOGNAME and PATH
# from the account of the real UID. A new real UID without an account falls
# back to the account of the original one.
proc reset_environment(candidates: List[Int]) [process, env, fs, error] -> Map[Str, Str] {
  var account: Account? = null
  for uid in candidates {
    if account == null {
      if let Ok(found) = user.by_uid(uid) { account = found }
    }
  }
  var environment: Map[Str, Str] = {}
  if let entry = account {
    if let Ok(term) = env.get("TERM") { environment["TERM"] = term }
    environment["SHELL"] = if entry.shell == "" { "/bin/sh" } else { entry.shell }
    environment["HOME"] = entry.home.display()
    environment["USER"] = entry.name
    environment["LOGNAME"] = entry.name
    environment["PATH"] = if entry.uid == 0 { ROOT_PATH } else { USER_PATH }
    return environment
  }
  usage_fail("--reset-env needs an account for the real UID")
  environment
}

proc launch(words: List[Str], environment: Map[Str, Str]?) [fs, process, env, error] {
  let command = words[0]
  if command == "" {
    gnu.error("failed to execute : No such file or directory")
    exit 127
  }
  if command.find("/") != null {
    match fs.stat(fp"{command}", follow_symlinks: true) {
      Ok(meta) => {
        if meta.kind != "file" or meta.mode.bit_and(0o111) == 0 {
          gnu.error(f"failed to execute {command}: Permission denied")
          exit 126
        }
      }
      Err(failure) => {
        let missing = (failure.errno ?? gnu.errno(failure)) == 2
        gnu.error(f"failed to execute {command}: {gnu.strerror(failure)}")
        exit if missing { 127 } else { 126 }
      }
    }
  } else if let Err(failure) = process.which(command) {
    let missing = failure is NotFound
    gnu.error(f"failed to execute {command}: {if missing { "No such file or directory" } else { "Permission denied" }}")
    exit if missing { 127 } else { 126 }
  }
  let plan = process.command_argv(command, words)
  let replaced = if let variables = environment { unix.exec_env(plan, variables) } else { unix.exec(plan) }
  if let Err(failure) = replaced {
    gnu.error(f"failed to execute {command}: {gnu.strerror(failure)}")
    exit if (failure.errno ?? gnu.errno(failure)) == 2 { 127 } else { 126 }
  }
}

proc main(...argv: List[Str]) [fs, process, env, io, error] {
  let request = parse(argv)
  if request.dump > 0 {
    dump(request.dump)
    return
  }
  if request.list_caps > 0 {
    gnu.write_text(collect { for entry in capability.CAPABILITIES { yield base_name(entry) } }.join("\n") + "\n")
    return
  }
  let original_uid = match unix.id() {
    Ok(ids) => ids.uid
    Err(_) => 0
  }
  let init_ids = if let uid = request.init_user { init_group_ids(uid) } else { [] }
  let death_signal = if let choice = request.pdeathsig {
    if choice == "keep" { read_state().parent_death_signal } else if choice == "clear" { 0 } else { signal_number(choice) }
  } else { -1 }
  if let edits = request.bounding { apply_bounding(edits) }
  if let edits = request.securebits { apply_securebits(edits) }
  if request.no_new_privs {
    if let Err(failure) = linux.set_no_new_privs() { die_with("disallow granting new privileges failed", failure) }
  }
  # Dropping root empties the permitted set and the ambient set. Capabilities
  # are therefore edited after the UID change, from a permitted set that
  # keep_caps preserves across it; exec then keeps them through the ambient set.
  let editing = request.inheritable != null or request.ambient != null
  if editing and (request.ruid != null or request.euid != null) {
    if let Err(failure) = linux.set_keep_capabilities(true) { die_with("set keep capabilities failed", failure) }
  }
  apply_groups(request, init_ids)
  apply_gids(request)
  apply_uids(request)
  if let edits = request.inheritable { apply_inheritable(edits) }
  if let edits = request.ambient { apply_ambient(edits) }
  if death_signal >= 0 {
    if let Err(failure) = linux.set_parent_death_signal(death_signal) { die_with("set parent death signal failed", failure) }
  }
  if let pid = request.ptracer {
    if let Err(failure) = linux.set_ptracer(pid) { die_with("set ptracer", failure) }
  }
  let environment: Map[Str, Str]? = if request.reset_env {
    let candidates = if let uid = request.ruid { [uid, original_uid] } else { [original_uid] }
    reset_environment(candidates)
  } else { null }
  launch(request.command, environment)
}
