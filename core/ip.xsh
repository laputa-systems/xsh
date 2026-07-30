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

type IpOptions = {operands: List[Str]}

proc main(...argv: List[Str]) [process, error] {
  let opts: IpOptions = cli.applet(argv, {operands: {form: "...ARG"}})?
  let operands = opts.operands

  if operands.len() == 1 and (operands[0] == "addr" or operands[0] == "address") {
    print_addr("")?
  } else if operands.len() == 2 and (operands[0] == "addr" or operands[0] == "address") and operands[1] == "show" {
    print_addr("")?
  } else if operands.len() == 4 and (operands[0] == "addr" or operands[0] == "address") and operands[1] == "show" and operands[2] == "dev" {
    print_addr(operands[3])?
  } else if operands.len() == 3 and (operands[0] == "addr" or operands[0] == "address") and operands[1] == "dev" {
    print_addr(operands[2])?
  } else if operands.len() == 1 and operands[0] == "route" {
    print_route()?
  } else if operands.len() == 2 and operands[0] == "route" and operands[1] == "show" {
    print_route()?
  } else {
    return Err(AppletError.Usage("ip: expected addr or route"))
  }
}
