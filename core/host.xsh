#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

proc main(...argv: List[Str]) [net, error] {
  var query_type = ""
  var operands: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    match arg {
      "-t" | "--type" => {
        index += 1
        query_type = argv[index]
      }
      _ => {
        if arg.starts_with("-") {
          return Err(AppletError.Usage("host: unsupported option"))
        }

        operands = operands.push(arg)
      }
    }

    index += 1
  }

  if operands.len() == 0 or operands.len() > 2 {
    return Err(AppletError.Usage("host: expected NAME [SERVER]"))
  }

  let name = operands[0]
  let server = if operands.len() > 1 { operands[1] } else { "" }

  if query_type != "" {
    for item in dns.lookup(name, query_type, server)? {
      print f"${item.name} has ${item.record} ${item.value}"
    }
  } else if name.contains(".") and name.split(".").len() == 4 {
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
