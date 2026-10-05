#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

proc print_addr(filter: Str) [process, error] {
  for iface in linux.interfaces()? |> sort-by .name {
    continue when filter != "" and iface.name != filter
    print f"{iface.name}: mtu {iface.mtu} flags {iface.flags.join(",")}"

    if iface.mac != "" {
      print f"    link/ether {iface.mac}"
    }

    for addr in iface.addresses {
      print f"    {addr.family} {addr.addr}/{addr.prefix_len}"
    }
  }
}

proc print_route() [process, error] {
  for route in linux.routes()? {
    if route.gateway == "" or route.gateway == "0.0.0.0" or route.gateway == "::" {
      print f"{route.dst} dev {route.dev} metric {route.metric}"
    } else {
      print f"{route.dst} via {route.gateway} dev {route.dev} metric {route.metric}"
    }
  }
}

type IpOptions = {operands: List[Str]}

proc main(...argv: List[Str]) [process, error] {
  let opts: IpOptions = cli.applet(argv, {operands: {form: "...ARG"}})?
  let operands = opts.operands

  match operands {
    ["addr"] | ["address"] | ["addr", "show"] | ["address", "show"] => print_addr("")
    ["addr", "show", "dev", name] | ["address", "show", "dev", name] | ["addr", "dev", name] | ["address", "dev", name] => print_addr(
      name,
    )
    ["route"] | ["route", "show"] => print_route()
    else => return Err(AppletError.Usage("ip: expected addr or route"))
  }
}
