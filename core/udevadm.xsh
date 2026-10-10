#!/bin/xsh
##! `udevadm` over sysfs, the udev database, and the kernel uevent stream.
##!
##! `info`, `trigger`, `settle`, `monitor`, `control`, and `version` follow
##! eudev's output formats. Devices come from `lib.sys_udev`, which reads `sys`
##! and `run/udev` below the directory named by `XSH_UDEVADM_ROOT` (default
##! `/`), so every command can run against a synthetic tree. `info -d FILE`
##! and `settle -E FILE` take real file paths and ignore the root.
##!
##! Options that need a udev daemon or its netlink group are rejected by name
##! instead of being accepted without effect: `control` has no daemon to talk
##! to, and `monitor -u` and `-t` need the daemon's own event stream, which the
##! kernel uevent stream does not carry. Without `-u`, `monitor` prints kernel
##! uevents.
use lib.gnu
use lib.search
use lib.sys_udev as udev

# The version eudev reports (its systemd compatibility level), which scripts
# compare numerically.
const UDEV_VERSION = "251"

const TOP_HELP = """udevadm [--help] [--version] [--debug] COMMAND [COMMAND OPTIONS]

Send control commands or test the device manager.

Commands:
  info          Query sysfs or the udev database
  trigger       Request events from the kernel
  settle        Wait for pending udev events
  control       Control the udev daemon
  monitor       Listen to kernel and udev events
  hwdb          maintain the hardware database index
  test          Test an event run
  test-builtin  Test a built-in command
"""

const INFO_HELP = """udevadm info [OPTIONS] [DEVPATH|FILE]

Query sysfs or the udev database.

  -h --help                   Print this message
     --version                Print version of the program
  -q --query=TYPE             Query device information:
       name                     Name of device node
       symlink                  Pointing to node
       path                     sysfs device path
       property                 The device properties
       all                      All values
  -p --path=SYSPATH           sysfs device path used for query or attribute walk
  -n --name=NAME              Node or symlink name used for query or attribute walk
  -r --root                   Prepend dev directory to path names
  -a --attribute-walk         Print all key matches walking along the chain
                              of parent devices
  -d --device-id-of-file=FILE Print major:minor of device containing this file
  -x --export                 Export key/value pairs
  -P --export-prefix=NAME     Export the key name with a prefix
  -e --export-db              Export the content of the udev database
  -c --cleanup-db             Clean up the udev database
"""

const TRIGGER_HELP = """udevadm trigger OPTIONS

Request events from the kernel.

  -h --help                         Show this help
     --version                      Show package version
  -v --verbose                      Print the list of devices while running
  -n --dry-run                      Do not actually trigger the events
  -t --type=                        Type of events to trigger
          devices                     sysfs devices (default)
          subsystems                  sysfs subsystems and drivers
  -c --action=ACTION                Event action value, default is "change"
  -s --subsystem-match=SUBSYSTEM    Trigger devices from a matching subsystem
  -S --subsystem-nomatch=SUBSYSTEM  Exclude devices from a matching subsystem
  -a --attr-match=FILE[=VALUE]      Trigger devices with a matching attribute
  -A --attr-nomatch=FILE[=VALUE]    Exclude devices with a matching attribute
  -p --property-match=KEY=VALUE     Trigger devices with a matching property
  -g --tag-match=KEY=VALUE          Trigger devices with a matching property
  -y --sysname-match=NAME           Trigger devices with this /sys path
     --name-match=NAME              Trigger devices with this /dev name
  -b --parent-match=NAME            Trigger devices with that parent device
"""

const SETTLE_HELP = """udevadm settle OPTIONS

Wait for pending udev events.

  -h --help                 Show this help
     --version              Show package version
  -t --timeout=SECONDS      Maximum time to wait for events
  -E --exit-if-exists=FILE  Stop waiting if file exists
"""

const MONITOR_HELP = """udevadm monitor [--property] [--kernel] [--udev] [--help]

Listen to kernel and udev events.

  -h --help                                Show this help
     --version                             Show package version
  -p --property                            Print the event properties
  -k --kernel                              Print kernel uevents
  -u --udev                                Print udev events
  -s --subsystem-match=SUBSYSTEM[/DEVTYPE] Filter events by subsystem
  -t --tag-match=TAG                       Filter events by tag
"""

const CONTROL_HELP = """udevadm control COMMAND

Control the udev daemon.

  -h --help                Show this help
     --version             Show package version
  -e --exit                Instruct the daemon to cleanup and exit
  -l --log-priority=LEVEL  Set the udev log level for the daemon
  -s --stop-exec-queue     Do not execute events, queue only
  -S --start-exec-queue    Execute events, flush queue
  -R --reload              Reload rules and databases
  -p --property=KEY=VALUE  Set a global property for all events
  -m --children-max=N      Maximum number of children
     --timeout=SECONDS     Maximum time to block for a reply
"""

const NO_DAEMON = "there is no udev daemon to control"

const WALK_HEADER = "\nUdevadm info starts with the device specified by the devpath and then\nwalks up the chain of parent devices. It prints for every device\nfound, all possible attributes in the udev rules key format.\nA rule to match, can be composed by the attributes of the device\nand the attributes from one single parent device.\n\n"

# Prints a message that eudev prints bare, without a program prefix, and exits.
proc die(message: Str, status: Int) [process] {
  eprint $message
  exit status
}

proc open_root() [fs, env, process, error] -> Result[FsRoot, Error] {
  fs.open_root(udev.root_path())
}

pure render_all(device: udev.Device) -> Str {
  var lines: List[Str] = [f"P: {device.devpath}"]
  if device.devnode != null {
    lines += [f"N: {device.devnode}"]
  }

  if device.link_priority != 0 {
    lines += [f"L: {device.link_priority}"]
  }

  for link in device.symlinks {
    lines += [f"S: {link}"]
  }

  for property in device.properties {
    lines += [f"E: {property.name}={property.value}"]
  }

  f"{lines.join("\n")}\n\n"
}

pure render_attribute_block(device: udev.Device, attributes: List[udev.Attribute], parent: Bool) -> Str {
  let suffix = if parent { "S" } else { "" }
  var lines: List[Str] = [
    f"  looking at {if parent { "parent " } else { "" }}device '{device.devpath}':",
    f"    KERNEL{suffix}==\"{device.sysname}\"",
    f"    SUBSYSTEM{suffix}==\"{device.subsystem ?? ""}\"",
    f"    DRIVER{suffix}==\"{device.driver ?? ""}\"",
  ]
  for attribute in attributes {
    lines += [f"    ATTR{suffix}{{{attribute.name}}}==\"{attribute.value}\""]
  }

  f"{lines.join("\n")}\n\n"
}

type InfoOptions = {
  query: Str?,
  path: Str?,
  name: Str?,
  root: Bool,
  attribute_walk: Bool,
  device_id_of_file: Str?,
  export: Bool,
  export_prefix: Str?,
  export_db: Bool,
  cleanup_db: Bool,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

# Resolves the device that -p, -n, or the operand selects. The three selectors
# are alternatives; eudev reports a second one as "device already specified".
proc select_device(root: FsRoot, opts: InfoOptions) [fs, env, io, process, error] -> Result[udev.Device, Error] {
  var selectors = 0
  if opts.path != null {
    selectors += 1
  }

  if opts.name != null {
    selectors += 1
  }

  if ! opts.operands.is_empty() {
    selectors += 1
  }

  if selectors > 1 {
    die("device already specified", 2)
  }

  if opts.operands.len() > 1 {
    gnu.extra_operand(opts.operands[1])
  }

  var sysfs_path: Str? = null
  var node_name: Str? = null
  if opts.path != null {
    sysfs_path = udev.sysfs_relative(opts.path)
  } else if opts.name != null {
    node_name = opts.name
  } else if ! opts.operands.is_empty() {
    let operand = opts.operands[0]
    if operand.starts_with("/dev/") {
      node_name = operand
    } else if udev.is_sysfs_path(operand) {
      sysfs_path = udev.sysfs_relative(operand)
    } else {
      die("Unknown device, --name=, --path=, or absolute path in /dev/ or /sys expected.", 4)
    }
  } else {
    gnu.help(INFO_HELP)
    exit 2
  }

  var devpath: Str? = null
  if node_name != null {
    let given = node_name
    let relative = if given.starts_with("/dev/") { given.byte_slice(5) } else { given }
    if relative.starts_with("/") {
      die("device node not found", 2)
    }

    devpath = udev.find_by_name(root, relative)?
    if devpath == null {
      die("device node not found", 2)
    }
  } else {
    devpath = udev.canonical_devpath(root, sysfs_path ?? "")?
    if devpath == null {
      die("syspath not found", 2)
    }
  }

  let device = udev.load(root, devpath ?? "")?
  if device == null {
    die("syspath not found", 2)
  }

  Ok(device ?? {
    devpath: "",
    sysname: "",
    subsystem: null,
    driver: null,
    devnode: null,
    major: null,
    minor: null,
    symlinks: [],
    link_priority: 0,
    tags: [],
    properties: [],
  })
}

# Removes the runtime database the way eudev does: the queue marker, data
# records that are not sticky, and the link, tag, and watch directories.
proc remove_database(root: FsRoot) [fs, process, error] {
  let host = root.host_path()?
  var failed = false
  var doomed: List[Path] = []
  if root.exists(p"run/udev/queue.bin") {
    doomed += [p"run/udev/queue.bin"]
  }

  for directory in [p"run/udev/data", p"run/udev/links", p"run/udev/tags", p"run/udev/static_node-tags", p"run/udev/watch"] {
    let listing = root.children(directory, max_entries: 65536)?
    for entry in listing.children {
      if directory == p"run/udev/data" {
        # eudev keeps data records whose sticky bit marks them as persistent.
        continue when fs.sticky(root.stat(entry, follow_symlinks: false)?.mode)
      }

      doomed += [entry]
    }
  }

  for entry in doomed {
    let target = fp"{host}/{entry}"
    if let Err(failure) = target.remove() {
      gnu.error(f"cannot remove '{target}': {gnu.strerror(failure)}")
      failed = true
    }
  }

  if failed {
    exit 1
  }
}

proc info(argv: List[Str]) [fs, env, process, io, error] -> Result[Unit, Error] {
  let opts: InfoOptions = cli.applet(argv, {
    gnu: {prog: "udevadm info", status: 1},
    query: {form: "-q --query TYPE"},
    path: {form: "-p --path SYSPATH"},
    name: {form: "-n --name NAME"},
    root: {form: "-r --root", default: false},
    attribute_walk: {form: "-a --attribute-walk", default: false},
    device_id_of_file: {form: "-d --device-id-of-file FILE"},
    export: {form: "-x --export", default: false},
    export_prefix: {form: "-P --export-prefix NAME"},
    export_db: {form: "-e --export-db", default: false},
    cleanup_db: {form: "-c --cleanup-db", default: false},
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    operands: {form: "...DEVICE"},
  })?
  if opts.help {
    gnu.help(INFO_HELP)
    return
  }

  if opts.version {
    gnu.write_text(f"{UDEV_VERSION}\n")
    return
  }

  var actions = 0
  for chosen in [opts.attribute_walk, opts.query != null, opts.device_id_of_file != null, opts.export_db, opts.cleanup_db] {
    if chosen {
      actions += 1
    }
  }

  if actions > 1 {
    gnu.usage_error("options --attribute-walk, --query, --device-id-of-file, --export-db, and --cleanup-db are mutually exclusive")
  }

  let query = opts.query ?? "all"
  if query not in ["name", "symlink", "path", "property", "all"] {
    die("unknown query type", 3)
  }

  if opts.export_prefix != null and ! opts.export {
    gnu.usage_error("option '--export-prefix' requires '--export'")
  }

  if opts.export and query != "property" {
    gnu.usage_error("option '--export' applies only to '--query=property'")
  }

  if opts.root and query not in ["name", "symlink"] {
    gnu.usage_error("option '--root' applies only to '--query=name' and '--query=symlink'")
  }

  let selects_device = opts.path != null or opts.name != null or ! opts.operands.is_empty()
  if selects_device and (opts.device_id_of_file != null or opts.export_db or opts.cleanup_db) {
    gnu.usage_error("a device cannot be selected with --device-id-of-file, --export-db, or --cleanup-db")
  }

  if opts.device_id_of_file != null {
    let file = opts.device_id_of_file
    match fs.stat(fp"{file}") {
      Ok(facts) => gnu.write_text(f"{fs.dev_major(facts.dev)}:{fs.dev_minor(facts.dev)}\n")
      Err(failure) => {
        gnu.cannot_access(file, failure)
        exit 1
      }
    }

    return
  }

  let root = open_root()?
  defer root.close()
  if opts.cleanup_db {
    remove_database(root)
    return
  }

  if opts.export_db {
    for devpath in udev.enumerate(root)? {
      if let device = udev.load(root, devpath)? {
        gnu.write_text(render_all(device))
      }
    }

    return
  }

  let device = select_device(root, opts)?
  if opts.attribute_walk {
    var output = WALK_HEADER
    output += render_attribute_block(device, udev.attributes(root, device.devpath)?, false)
    var current = device.devpath
    while true {
      let parent_path = udev.parent_devpath(root, current)?
      break when parent_path == null

      current = parent_path
      if let parent = udev.load(root, current)? {
        output += render_attribute_block(parent, udev.attributes(root, current)?, true)
      }
    }

    gnu.write_text(output)
    return
  }

  if query == "name" {
    if device.devnode == null {
      die("no device node found", 5)
    }

    gnu.write_text(f"{if opts.root { "/dev/" } else { "" }}{device.devnode ?? ""}\n")
  } else if query == "symlink" {
    let links = device.symlinks |> map { |link| if opts.root { f"/dev/{link}" } else { link } }
    gnu.write_text(f"{links.join(" ")}\n")
  } else if query == "path" {
    gnu.write_text(f"{device.devpath}\n")
  } else if query == "property" {
    var output = ""
    let prefix = opts.export_prefix ?? ""
    for property in device.properties {
      output += if opts.export { f"{prefix}{property.name}='{property.value}'\n" } else { f"{property.name}={property.value}\n" }
    }

    gnu.write_text(output)
  } else {
    gnu.write_text(render_all(device))
  }
}

type TriggerOptions = {
  verbose: Bool,
  dry_run: Bool,
  kind: Str,
  action: Str,
  subsystem_match: List[Str],
  subsystem_nomatch: List[Str],
  attr_match: List[Str],
  attr_nomatch: List[Str],
  property_match: List[Str],
  tag_match: List[Str],
  sysname_match: List[Str],
  name_match: List[Str],
  parent_match: Str?,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

pure matches_any(patterns: List[Str], text: Str) -> Bool {
  for pattern in patterns {
    return true when search.glob(pattern, bytes.from_text(text))
  }

  false
}

# True when every FILE[=VALUE] selector is satisfied. A selector without a
# value requires only that the attribute is readable; with a value, the
# attribute text (one trailing newline removed) must match it as a glob.
proc attribute_selected(attributes: List[udev.Attribute], selector: Str) -> Bool {
  let pair = selector.split("=", maxsplit: 1)
  for attribute in attributes {
    if attribute.name == pair[0] {
      return pair.len() == 1 or search.glob(pair[1], bytes.from_text(attribute.value))
    }
  }

  false
}

pure property_selected(device: udev.Device, selector: Str) -> Bool {
  let pair = selector.split("=", maxsplit: 1)
  for property in device.properties {
    if search.glob(pair[0], bytes.from_text(property.name)) and search.glob(pair[1], bytes.from_text(property.value)) {
      return true
    }
  }

  false
}

# Maps a trigger operand, --parent-match, or --name-match value to a canonical
# device path: `/dev/NAME` through the device node name, anything else as a
# sysfs path (with or without the `/sys` prefix).
proc device_argument(root: FsRoot, given: Str, shown: Str) [fs, process, error] -> Result[Str, Error] {
  var devpath: Str? = null
  if given.starts_with("/dev/") {
    devpath = udev.find_by_name(root, given.byte_slice(5))?
  } else {
    devpath = udev.canonical_devpath(root, udev.sysfs_relative(given))?
  }

  if devpath == null {
    die(f"unable to open the device '{shown}'", 2)
  }

  Ok(devpath ?? "")
}

proc trigger(argv: List[Str]) [fs, env, process, io, error] {
  let opts: TriggerOptions = cli.applet(argv, {
    gnu: {prog: "udevadm trigger", status: 1},
    verbose: {form: "-v --verbose", default: false},
    dry_run: {form: "-n --dry-run", default: false},
    kind: {form: "-t --type TYPE", default: "devices"},
    action: {form: "-c --action ACTION", default: "change"},
    subsystem_match: {form: "-s --subsystem-match SUBSYSTEM", repeated: true},
    subsystem_nomatch: {form: "-S --subsystem-nomatch SUBSYSTEM", repeated: true},
    attr_match: {form: "-a --attr-match FILE", repeated: true},
    attr_nomatch: {form: "-A --attr-nomatch FILE", repeated: true},
    property_match: {form: "-p --property-match KEY", repeated: true},
    tag_match: {form: "-g --tag-match TAG", repeated: true},
    sysname_match: {form: "-y --sysname-match NAME", repeated: true},
    name_match: {form: "--name-match NAME", repeated: true},
    parent_match: {form: "-b --parent-match NAME"},
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    operands: {form: "...DEVICE"},
  })?
  if opts.help {
    gnu.help(TRIGGER_HELP)
    return
  }

  if opts.version {
    gnu.write_text(f"{UDEV_VERSION}\n")
    return
  }

  if opts.kind not in ["devices", "subsystems"] {
    die(f"unknown type --type={opts.kind}", 2)
  }

  if opts.action not in ["add", "remove", "change"] {
    die(f"unknown action '{opts.action}'", 2)
  }

  for selector in opts.property_match {
    if "=" not in selector {
      gnu.usage_error(f"property match {gnu.quote(selector)} must be KEY=VALUE")
    }
  }

  for selector in opts.tag_match {
    if selector == "" {
      gnu.usage_error("tag match must not be empty")
    }
  }

  let root = open_root()?
  defer root.close()
  var targets: List[Str] = []
  if opts.kind == "subsystems" {
    let unsupported = [@opts.attr_match, @opts.attr_nomatch, @opts.property_match, @opts.tag_match, @opts.sysname_match, @opts.name_match, @opts.operands]
    if ! unsupported.is_empty() or opts.parent_match != null {
      gnu.usage_error("only --subsystem-match and --subsystem-nomatch apply to --type=subsystems")
    }

    for entry in udev.subsystems(root)? {
      let included = opts.subsystem_match.is_empty() or matches_any(opts.subsystem_match, entry.kind)
      continue when ! included or matches_any(opts.subsystem_nomatch, entry.kind)

      targets += [entry.syspath]
    }
  } else if ! opts.operands.is_empty() or ! opts.name_match.is_empty() {
    # Named devices are triggered as given; the match options select among
    # scanned devices and do not narrow an explicit list.
    for given in opts.operands {
      targets += [device_argument(root, given, given)?]
    }

    # --name-match names a node below /dev, with or without the prefix.
    for given in opts.name_match {
      targets += [device_argument(root, if given.starts_with("/dev/") { given } else { f"/dev/{given}" }, given)?]
    }
  } else {
    var parent: Str? = null
    if opts.parent_match != null {
      parent = device_argument(root, opts.parent_match, opts.parent_match)?
    }

    for devpath in udev.enumerate(root)? {
      if parent != null {
        let top = parent
        continue when devpath != top and ! devpath.starts_with(f"{top}/")
      }

      let device = udev.load(root, devpath)?
      continue when device == null

      let facts = device ?? {devpath: "", sysname: "", subsystem: null, driver: null, devnode: null, major: null, minor: null, symlinks: [], link_priority: 0, tags: [], properties: []}
      let subsystem = facts.subsystem ?? ""
      if ! opts.subsystem_match.is_empty() {
        continue when ! matches_any(opts.subsystem_match, subsystem)
      }

      continue when matches_any(opts.subsystem_nomatch, subsystem)

      if ! opts.sysname_match.is_empty() {
        continue when ! matches_any(opts.sysname_match, facts.sysname)
      }

      var rejected = false
      for tag in opts.tag_match {
        if tag not in facts.tags {
          rejected = true
        }
      }

      if ! opts.property_match.is_empty() {
        var any_property = false
        for selector in opts.property_match {
          if property_selected(facts, selector) {
            any_property = true
          }
        }

        if ! any_property {
          rejected = true
        }
      }

      if ! opts.attr_match.is_empty() or ! opts.attr_nomatch.is_empty() {
        let attributes = udev.attributes(root, devpath)?
        for selector in opts.attr_match {
          if ! attribute_selected(attributes, selector) {
            rejected = true
          }
        }

        for selector in opts.attr_nomatch {
          if attribute_selected(attributes, selector) {
            rejected = true
          }
        }
      }

      continue when rejected

      targets += [devpath]
    }
  }

  var failed = false
  for target in targets {
    if opts.verbose {
      gnu.write_text(f"/sys{target}\n")
    }

    continue when opts.dry_run

    let uevent = fp"sys{target}/uevent"
    continue unless root.exists(uevent)

    if let Err(failure) = root.write(uevent, opts.action) {
      gnu.error(f"cannot trigger '/sys{target}': {gnu.strerror(failure)}")
      failed = true
    }
  }

  if failed {
    exit 1
  }
}

type SettleOptions = {timeout: Str?, exit_if_exists: Str?, help: Bool, version: Bool, operands: List[Str]}

proc settle(argv: List[Str]) [fs, env, process, io, time, error] -> Result[Unit, Error] {
  let opts: SettleOptions = cli.applet(argv, {
    gnu: {prog: "udevadm settle", status: 1},
    timeout: {form: "-t --timeout SECONDS"},
    exit_if_exists: {form: "-E --exit-if-exists FILE"},
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    operands: {form: "...EXTRA"},
  })?
  if opts.help {
    gnu.help(SETTLE_HELP)
    return
  }

  if opts.version {
    gnu.write_text(f"{UDEV_VERSION}\n")
    return
  }

  if ! opts.operands.is_empty() {
    die(f"Extraneous argument: '{opts.operands[0]}'", 1)
  }

  var seconds = 120
  if opts.timeout != null {
    let text = opts.timeout ?? ""
    match text.parse_uint() {
      Ok(value) => seconds = value
      Err(_) => {
        let reason = if text.starts_with("-") and text.byte_len() > 1 { "Result not representable" } else { "Invalid argument" }
        die(f"Invalid timeout value '{text}': {reason}", 1)
      }
    }
  }

  let root = open_root()?
  defer root.close()
  # A root caller first pings the daemon's control socket; without a daemon
  # nothing will ever drain the queue, so settle has nothing to wait for.
  return when unix.id()?.euid == 0 and ! udev.daemon_present(root)?

  let deadline = time.now() + seconds * 1000
  while true {
    if let marker = opts.exit_if_exists {
      return when fp"{marker}".exists()?
    }

    return when ! udev.queue_pending(root)?

    if time.now() >= deadline {
      exit 1
    }

    time.sleep(time.millis(50))?
  }
}

type MonitorOptions = {property: Bool, kernel: Bool, subsystem_match: List[Str], help: Bool, version: Bool, operands: List[Str]}

# The kernel event stream reports DEVTYPE only inside the environment, so a
# `SUBSYSTEM/DEVTYPE` filter reads it from there.
pure monitor_selected(event: LinuxUevent, filters: List[Str]) -> Bool {
  return true when filters.is_empty()

  var devtype = ""
  for entry in event.env {
    if entry.name == "DEVTYPE" {
      devtype = entry.value
    }
  }

  for filter in filters {
    let pair = filter.split("/", maxsplit: 1)
    if pair[0] == event.subsystem and (pair.len() == 1 or pair[1] == devtype) {
      return true
    }
  }

  false
}

proc monitor(argv: List[Str]) [env, process, io, time, error] -> Result[Unit, Error] {
  let opts: MonitorOptions = cli.applet(argv, {
    gnu: {
      prog: "udevadm monitor",
      status: 1,
      unsupported: {
        "-u": "udev events are published by the udev daemon, which the kernel uevent stream does not carry",
        "--udev": "udev events are published by the udev daemon, which the kernel uevent stream does not carry",
        "-t": "tags are attached by the udev daemon, which the kernel uevent stream does not carry",
        "--tag-match": "tags are attached by the udev daemon, which the kernel uevent stream does not carry",
      },
    },
    property: {form: "-p --property", default: false},
    kernel: {form: "-k --kernel", default: false},
    subsystem_match: {form: "-s --subsystem-match SUBSYSTEM", repeated: true},
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    operands: {form: "...EXTRA"},
  })?
  if opts.help {
    gnu.help(MONITOR_HELP)
    return
  }

  if opts.version {
    gnu.write_text(f"{UDEV_VERSION}\n")
    return
  }

  if ! opts.operands.is_empty() {
    die(f"Extraneous argument: '{opts.operands[0]}'", 1)
  }

  let events = linux.uevent_stream()?
  gnu.write_text("monitor will print the received events for:\nKERNEL - the kernel uevent\n\n")
  # Event stamps are seconds since boot. Uptime is whole seconds and the wall
  # clock is milliseconds, so stamps advance with millisecond resolution from
  # the uptime observed at start.
  let base_ms = unix.uptime_seconds()? * 1000
  let started = time.now()
  for event in events {
    continue when ! monitor_selected(event, opts.subsystem_match)

    let stamp = base_ms + time.now() - started
    let micros = stamp % 1000 * 1000
    gnu.write_text(f"{"KERNEL":<6}[{stamp / 1000}.{micros:06}] {event.action:<8} {event.devpath} ({event.subsystem})\n")
    if opts.property {
      var lines = ""
      for entry in event.env |> sort-by .name {
        lines += f"{entry.name}={entry.value}\n"
      }

      gnu.write_text(f"{lines}\n")
    }
  }
}

type ControlOptions = {help: Bool, version: Bool, operands: List[Str]}

proc control(argv: List[Str]) [env, process, io, error] -> Result[Unit, Error] {
  let opts: ControlOptions = cli.applet(argv, {
    gnu: {
      prog: "udevadm control",
      status: 1,
      unsupported: {
        "-e": NO_DAEMON,
        "--exit": NO_DAEMON,
        "-l": NO_DAEMON,
        "--log-priority": NO_DAEMON,
        "--log-level": NO_DAEMON,
        "-s": NO_DAEMON,
        "--stop-exec-queue": NO_DAEMON,
        "-S": NO_DAEMON,
        "--start-exec-queue": NO_DAEMON,
        "-R": NO_DAEMON,
        "--reload": NO_DAEMON,
        "-p": NO_DAEMON,
        "--property": NO_DAEMON,
        "-m": NO_DAEMON,
        "--children-max": NO_DAEMON,
        "--timeout": NO_DAEMON,
      },
    },
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    operands: {form: "...EXTRA"},
  })?
  if opts.help {
    gnu.help(CONTROL_HELP)
    return
  }

  if opts.version {
    gnu.write_text(f"{UDEV_VERSION}\n")
    return
  }

  if ! opts.operands.is_empty() {
    die(f"Extraneous argument: '{opts.operands[0]}'", 1)
  }

  die("Option missing", 1)
}

proc main(...argv: List[Str]) {
  var rest = argv
  var debug = false
  while ! rest.is_empty() and rest[0].starts_with("-") and rest[0] != "-" {
    let option = rest[0]
    if option == "-h" or option == "--help" {
      gnu.help(TOP_HELP)
      return
    } else if option == "-V" or option == "--version" {
      gnu.write_text(f"{UDEV_VERSION}\n")
      return
    } else if option == "-d" or option == "--debug" {
      debug = true
      rest = rest |> drop(1)
    } else {
      eprint f"udevadm: unrecognized option {gnu.quote(option)}"
      exit 2
    }
  }

  if rest.is_empty() {
    die("udevadm: missing or unknown command", 2)
  }

  let command = rest[0]
  let arguments = rest |> drop(1)
  if debug {
    eprint f"calling: {command}"
  }

  match command {
    "info" => info(arguments)?
    "trigger" => trigger(arguments)?
    "settle" => settle(arguments)?
    "monitor" => monitor(arguments)?
    "control" => control(arguments)?
    "version" => gnu.write_text(f"{UDEV_VERSION}\n")
    "help" => gnu.help(TOP_HELP)
    "hwdb" | "test" | "test-builtin" => die(f"udevadm: command {gnu.quote(command)} is not supported: it needs the udev rules engine", 1)
    else => die("udevadm: missing or unknown command", 2)
  }
}
