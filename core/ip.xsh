#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

proc print_addr(filter: Str) [process, error] {
  for iface in linux.interfaces()? |> sort-by .name {
    continue when filter != "" and iface.name != filter
    print f"${iface.name}: mtu ${iface.mtu} flags ${iface.flags.join(",")}"

    if iface.mac != "" {
      print f"    link/ether ${iface.mac}"
    }

    for addr in iface.addresses {
      print f"    ${addr.family} ${addr.addr}/${addr.prefix_len}"
    }
  }
}

proc print_route() [process, error] {
  for route in linux.routes()? {
    if route.gateway == "" or route.gateway == "0.0.0.0" or route.gateway == "::" {
      print f"${route.dst} dev ${route.dev} metric ${route.metric}"
    } else {
      print f"${route.dst} via ${route.gateway} dev ${route.dev} metric ${route.metric}"
    }
  }
}

proc main(...argv: List[Str]) [process, error] {
  if argv.len() == 1 and (argv[0] == "addr" or argv[0] == "address") {
    print_addr("")?
  } else if argv.len() == 2 and (argv[0] == "addr" or argv[0] == "address") and argv[1] == "show" {
    print_addr("")?
  } else if argv.len() == 4 and (argv[0] == "addr" or argv[0] == "address") and argv[1] == "show" and argv[2] == "dev" {
    print_addr(argv[3])?
  } else if argv.len() == 3 and (argv[0] == "addr" or argv[0] == "address") and argv[1] == "dev" {
    print_addr(argv[2])?
  } else if argv.len() == 1 and argv[0] == "route" {
    print_route()?
  } else if argv.len() == 2 and argv[0] == "route" and argv[1] == "show" {
    print_route()?
  } else {
    return Err(AppletError.Usage("ip: expected addr or route"))
  }
}
