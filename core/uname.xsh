#!/bin/xsh
use lib.gnu

const USAGE = """Usage: uname [OPTION]...
Print certain system information.  With no OPTION, same as -s.

  -a, --all                print all information, in the following order,
                             except omit -p and -i if unknown
  -A, --all-labeled        print all information, one labeled field per line,
                             except omit -p and -i if unknown
  -s, --kernel-name        print the kernel name
  -n, --nodename           print the network node hostname
  -r, --kernel-release     print the kernel release
  -v, --kernel-version     print the kernel version
  -m, --machine            print the machine hardware name
  -p, --processor          print the processor type (non-portable)
  -i, --hardware-platform  print the hardware platform (non-portable)
  -o, --operating-system   print the operating system
      --help        display this help and exit
      --version     output version information and exit
"""

type Field = {label: Str, text: Str}

type UnameOptions = {
  all: Bool,
  all_labeled: Bool,
  kernel_name: Bool,
  nodename: Bool,
  kernel_release: Bool,
  kernel_version: Bool,
  machine: Bool,
  processor: Bool,
  hardware_platform: Bool,
  operating_system: Bool,
  help: Bool,
  version: Bool,
}

# Processor and hardware platform are never determined, so like GNU the
# applet prints `unknown` for them only when asked explicitly and omits them
# from -a and -A. The operating system is GNU's fixed `GNU/Linux`.
proc main(...argv: List[Str]) [process, env, io, error] {
  let opts: UnameOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      all: {form: "-a --all", default: false, conflicts: ["all_labeled"]},
      all_labeled: {form: "-A --all-labeled", default: false, conflicts: ["all"]},
      kernel_name: {form: "-s --kernel-name --sysname", default: false},
      nodename: {form: "-n --nodename", default: false},
      kernel_release: {form: "-r --kernel-release --release", default: false},
      kernel_version: {form: "-v --kernel-version", default: false},
      machine: {form: "-m --machine", default: false},
      processor: {form: "-p --processor", default: false},
      hardware_platform: {form: "-i --hardware-platform", default: false},
      operating_system: {form: "-o --operating-system", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("uname")
    return
  }

  let everything = opts.all or opts.all_labeled
  let chosen = everything or opts.kernel_name or opts.nodename or opts.kernel_release or opts.kernel_version or opts.machine or opts.processor or opts.hardware_platform or opts.operating_system
  let info = system.uname()?
  var fields: List[Field] = []

  if everything or opts.kernel_name or ! chosen {
    fields += [{label: "Kernel name", text: info.sysname}]
  }

  if everything or opts.nodename {
    fields += [{label: "Node name", text: info.nodename}]
  }

  if everything or opts.kernel_release {
    fields += [{label: "Kernel release", text: info.release}]
  }

  if everything or opts.kernel_version {
    fields += [{label: "Kernel version", text: info.version}]
  }

  if everything or opts.machine {
    fields += [{label: "Machine", text: info.machine}]
  }

  if opts.processor and ! everything {
    fields += [{label: "Processor", text: "unknown"}]
  }

  if opts.hardware_platform and ! everything {
    fields += [{label: "Hardware platform", text: "unknown"}]
  }

  if everything or opts.operating_system {
    fields += [{label: "Operating system", text: "GNU/Linux"}]
  }

  if opts.all_labeled {
    gnu.write_text(f"{[f"{field.label}: {field.text}\n" for field in fields].join("")}")
  } else {
    gnu.write_text(f"{[field.text for field in fields].join(" ")}\n")
  }
}
