#!/bin/xsh
use lib.gnu

const USAGE = """Usage: hostname [OPTION]... [HOSTNAME]
Display or set the system's host name.

  -s, --short       display the short host name (the part before the first dot)
  -d, --domain      display the DNS domain name
  -i, --ip-address  display the network address(es) of the host name
  -f, --fqdn        display the fully qualified domain name
      --help        display this help and exit
      --version     output version information and exit
"""

type HostnameOptions = {
  short: Bool,
  domain: Bool,
  ip_address: Bool,
  fqdn: Bool,
  help: Bool,
  version: Bool,
  names: List[Str],
}

# The part of NAME after its first dot, or "" without one.
pure domain_of(name: Str) -> Str {
  let at = name.find(".") ?? -1

  return "" when at < 0

  name.byte_slice(at + 1)
}

# The canonical name the resolver reports for the host, or the plain host name
# when the lookup fails or names nothing more specific.
proc canonical_name(name: Str) [net, error] -> Str {
  guard let records = dns.resolve_host(name) else {
    return name
  }

  for record in records {
    return record.name when record.name != ""
  }

  name
}

proc main(...argv: List[Str]) [process, env, io, net, error] {
  let opts: HostnameOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      short: {form: "-s --short", default: false, conflicts: ["domain", "ip_address", "fqdn"]},
      domain: {form: "-d --domain", default: false, conflicts: ["short", "ip_address", "fqdn"]},
      ip_address: {form: "-i --ip-address", default: false, conflicts: ["short", "domain", "fqdn"]},
      fqdn: {form: "-f --fqdn", default: false, conflicts: ["short", "domain", "ip_address"]},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      names: {form: "...HOSTNAME"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("hostname")
    return
  }

  if opts.names.len() > 1 {
    gnu.extra_operand(opts.names[1])
  }

  if opts.names.len() == 1 {
    if opts.short or opts.domain or opts.ip_address or opts.fqdn {
      gnu.usage_error("no options can be used when setting the host name")
    }

    if let Err(failure) = unix.set_hostname(opts.names[0]) {
      gnu.error(if gnu.errno(failure) == 1 { "you must be root to change the host name" } else { gnu.strerror(failure) })
      abort(1)
    }

    return
  }

  let name = system.hostname()?

  if opts.ip_address {
    var seen: List[Str] = []

    for record in dns.resolve_host(name)? {
      if ! (record.addr in seen) {
        seen += [record.addr]
      }
    }

    gnu.write_text(f"{seen.join(" ")}\n")
  } else if opts.short {
    gnu.write_text(f"{name.split(".")[0]}\n")
  } else if opts.domain {
    let domain = domain_of(canonical_name(name))

    if domain != "" {
      gnu.write_text(f"{domain}\n")
    }
  } else if opts.fqdn {
    gnu.write_text(f"{canonical_name(name)}\n")
  } else {
    gnu.write_text(f"{name}\n")
  }
}
