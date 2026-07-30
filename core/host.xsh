#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type HostOptions = {query_type: Str, operands: List[Str]}

proc main(...argv: List[Str]) [net, error] {
  let opts: HostOptions = cli.applet(
    argv,
    {
      query_type: {
        form: "-t --type TYPE",
        default: "",
      },
      operands: {
        form: "...ARG",
      },
    },
  )?
  let query_type = opts.query_type
  let operands = opts.operands

  if operands.len() == 0 or operands.len() > 2 {
    return Err(AppletError.Usage("host: expected NAME [SERVER]"))
  }

  let name = operands[0]
  let server = if operands.len() > 1 { operands[1] } else { "" }

  if query_type != "" {
    for item in dns.lookup(name, query_type, server)? {
      print f"${item.name} has ${item.record} ${item.value}"
    }
  } else if "." in name and name.split(".").len() == 4 {
    for item in dns.reverse(name)? {
      print f"${name} domain name pointer ${item}"
    }
  } else if server != "" {
    for item in dns.lookup(name, "A", server)? {
      print f"${item.name} ${item.record} ${item.value}"
    }

    for item in dns.lookup(name, "AAAA", server)? {
      print f"${item.name} ${item.record} ${item.value}"
    }
  } else {
    for item in dns.resolve_host(name)? {
      print f"${item.name} ${item.family} ${item.addr}"
    }
  }
}
