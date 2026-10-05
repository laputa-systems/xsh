#!/bin/xsh
use lib.gnu

const USAGE = """Usage: hostid [OPTION]
Print the numeric identifier (in hexadecimal) for the current host.

      --help     display this help and exit
      --version  output version information and exit
"""

type HostidOptions = {help: Bool, version: Bool}

# The identifier gethostid() reports without a network lookup: the four bytes
# of /etc/hostid in native (little-endian) order, or 0 when the file is absent
# or short, which is the musl result. glibc derives an id from the host's IPv4
# address instead (request: system.hostid).
proc host_identifier() [fs] -> Int {
  guard let data = p"/etc/hostid".read_bytes() else {
    return 0
  }

  return 0 when data.len() < 4

  bytes.unpack_le(data, 4) ?? 0
}

# Eight lowercase hexadecimal digits of a 32-bit value.
pure hex8(value: Int) -> Str {
  var out = ""
  var rest = value % 4294967296

  repeat 8 times {
    out = f"{"0123456789abcdef".byte_slice(rest % 16, length: 1)}{out}"
    rest = rest / 16
  }

  out
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: HostidOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("hostid")
    return
  }

  gnu.write_text(f"{hex8(host_identifier())}\n")
}
