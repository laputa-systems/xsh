#!/bin/xsh
use lib.net_state
use lib.nettools

error AppletError = Usage : Usage

const USAGE = "ip: expected addr, link or route"

pure link_line(link: net_state.Link) -> Str {
  f"{link.name}: mtu {link.mtu ?? 0} flags {net_state.link_flag_names(link.flags).join(",")}"
}

pure mac_text(link: net_state.Link) -> Str {
  guard let mac = link.mac else { return "" }
  nettools.address_text(mac, mac.len(), ":", false)
}

pure render_links(state: net_state.State, filter: Str, addresses: Bool) -> List[Str] {
  collect {
    for link in state.links |> sort-by .name {
      continue when filter != "" and link.name != filter
      yield link_line(link)
      let mac = mac_text(link)
      yield f"    link/ether {mac}" when mac != ""
      if addresses {
        for address in net_state.addresses_of(state, link.ifindex) {
          yield f"    {address.family} {address.local}/{address.prefix}"
        }
      }
    }
  }
}

pure link_names(state: net_state.State) -> Map[Int, Str] {
  var names: Map[Int, Str] = {}
  for link in state.links {
    names[link.ifindex] = link.name
  }
  names
}

pure render_routes(state: net_state.State) -> List[Str] {
  let names = link_names(state)
  collect {
    for route in state.routes {
      continue when route.table != net_state.TABLE_MAIN or net_state.is_cloned(route)
      let none = net_state.unspecified(route.family)
      let destination = if route.prefix == 0 { "default" } else { f"{route.destination ?? none}/{route.prefix}" }
      for hop in route.hops {
        let device = if let index = hop.ifindex { names.get(index) ?? "*" } else { "*" }
        let gateway = hop.gateway ?? none
        if gateway == none {
          yield f"{destination} dev {device} metric {route.priority}"
        } else {
          yield f"{destination} via {gateway} dev {device} metric {route.priority}"
        }
      }
    }
  }
}

proc observe() [process, env, error] -> Result[net_state.State, Error] {
  net_state.observe()?
}

proc show_links(filter: Str, addresses: Bool) [process, env, error] -> Result[Unit, Error] {
  for line in render_links(observe()?, filter, addresses) {
    print $line
  }
  Ok()
}

proc show_routes() [process, env, error] -> Result[Unit, Error] {
  for line in render_routes(observe()?) {
    print $line
  }
  Ok()
}

proc change(request: net_state.Change) [process, net, error] -> Result[Unit, Error] {
  net_state.apply(request)?
}

pure device(state: net_state.State, name: Str) -> Result[net_state.Link, Error] {
  for link in state.links {
    return Ok(link) when link.name == name
  }
  fail f"cannot find device \"{name}\""
}

pure family_of(text: Str) -> net_state.Family {
  if text.find(":") == null { .Inet } else { .Inet6 }
}

pure host_length(family: net_state.Family) -> Int {
  if family == .Inet { 32 } else { 128 }
}

type HostPrefix = {host: Str, family: net_state.Family, prefix: Int}

pure parsed_prefix(word: Str) -> Result[HostPrefix, Error] {
  guard let slash = word.find("/") else {
    let family = family_of(word)
    return Ok({host: word, family, prefix: host_length(family)})
  }
  let host = word.byte_slice(0, length: slash)
  let family = family_of(host)
  guard let prefix = nettools.decimal(word.byte_slice(slash + 1)) else { fail f"invalid prefix \"{word}\"" }
  fail f"invalid prefix \"{word}\"" when prefix > host_length(family)
  Ok({host, family, prefix})
}

proc link_set(name: Str, state_word: Str) [process, env, net, error] -> Result[Unit, Error] {
  let link = device(observe()?, name)?
  match state_word {
    "up" => change(net_state.LinkUp(link.ifindex))
    "down" => change(net_state.LinkDown(link.ifindex))
    else => error.fail(f"expected up or down, not \"{state_word}\"")
  }
}

proc address_change(add: Bool, word: Str, name: Str, broadcast: Str?) [process, env, net, error] -> Result[Unit, Error] {
  let state = observe()?
  let link = device(state, name)?
  let wanted = parsed_prefix(word)?
  if add {
    return change(net_state.AddAddress({ifindex: link.ifindex, family: wanted.family, local: wanted.host, prefix: wanted.prefix, peer: null, broadcast, scope: 0, flags: 0}))
  }
  let matches = [item for item in net_state.addresses_of(state, link.ifindex) if item.family == wanted.family and item.local == wanted.host and item.prefix == wanted.prefix]
  fail f"{word} is not an address of {name}" when matches.is_empty()
  change(net_state.RemoveAddress(matches[0]))
}

type RouteWords = {destination: Str, gateway: Str?, device: Str?, metric: Int}

pure route_words(words: List[Str]) -> Result[RouteWords, Error] {
  fail "expected a route destination" when words.is_empty()
  var found: RouteWords = {destination: words[0], gateway: null, device: null, metric: 0}
  var at = 1
  while at < words.len() {
    fail f"\"{words[at]}\" needs a value" when at + 1 >= words.len()
    let value = words[at + 1]
    match words[at] {
      "via" => found.gateway = value
      "dev" => found.device = value
      "metric" => {
        guard let metric = nettools.decimal(value) else { fail f"invalid metric \"{value}\"" }
        found.metric = metric
      }
      else => fail f"unknown route option \"{words[at]}\""
    }
    at += 2
  }
  Ok(found)
}

type RouteSelector = {family: net_state.Family, destination: Str?, prefix: Int, gateway: Str?, ifindex: Int?, metric: Int}

pure route_matches(item: net_state.Route, wanted: RouteSelector) -> Bool {
  return false when item.family != wanted.family or item.table != net_state.TABLE_MAIN or item.prefix != wanted.prefix or item.hops.len() != 1
  return false when wanted.prefix > 0 and item.destination != wanted.destination
  return false when wanted.metric != 0 and item.priority != wanted.metric
  let hop = item.hops[0]
  return false when wanted.gateway != null and hop.gateway != wanted.gateway
  wanted.ifindex == null or hop.ifindex == wanted.ifindex
}

pure route_selector(state: net_state.State, words: RouteWords) -> Result[RouteSelector, Error] {
  let ifindex = if let name = words.device { device(state, name)?.ifindex } else { null }
  if words.destination == "default" {
    let family: net_state.Family = if let via = words.gateway { family_of(via) } else { .Inet }
    return Ok({family, destination: null, prefix: 0, gateway: words.gateway, ifindex, metric: words.metric})
  }
  let target = parsed_prefix(words.destination)?
  Ok({family: target.family, destination: target.host, prefix: target.prefix, gateway: words.gateway, ifindex, metric: words.metric})
}

proc route_change(add: Bool, words: List[Str]) [process, env, net, error] -> Result[Unit, Error] {
  let state = observe()?
  let wanted = route_selector(state, route_words(words)?)?
  if !add {
    let matches = [item for item in state.routes if route_matches(item, wanted)]
    fail f"no route matches \"{words.join(" ")}\"" when matches.is_empty()
    return change(net_state.RemoveRoute(matches[0]))
  }
  fail "a route needs dev or via" when wanted.ifindex == null and wanted.gateway == null
  let c = linux.net_constants()
  let scope = if wanted.family == .Inet and wanted.gateway == null { c.RT_SCOPE_LINK } else { c.RT_SCOPE_UNIVERSE }
  change(
    net_state.AddRoute({
      family: wanted.family,
      table: net_state.TABLE_MAIN,
      destination: wanted.destination,
      prefix: wanted.prefix,
      source: null,
      source_prefix: 0,
      preferred_source: null,
      tos: 0,
      priority: wanted.metric,
      protocol: c.RTPROT_BOOT,
      scope,
      route_type: c.RTN_UNICAST,
      flags: 0,
      hops: [{ifindex: wanted.ifindex, gateway: wanted.gateway}],
      other_attributes: [],
    }),
  )
}

proc dispatch(operands: List[Str]) [process, env, net, error] -> Result[Unit, Error] {
  match operands {
    ["addr"] | ["address"] | ["addr", "show"] | ["address", "show"] => show_links("", true)
    ["addr", "show", "dev", name] | ["address", "show", "dev", name] | ["addr", "dev", name] | ["address", "dev", name] => show_links(name, true)
    ["addr", "add", word, "dev", name] | ["address", "add", word, "dev", name] => address_change(true, word, name, null)
    ["addr", "add", word, "broadcast", broadcast, "dev", name] | ["address", "add", word, "broadcast", broadcast, "dev", name] => address_change(true, word, name, broadcast)
    ["addr", "del", word, "dev", name] | ["address", "del", word, "dev", name] => address_change(false, word, name, null)
    ["link"] | ["link", "show"] => show_links("", false)
    ["link", "show", "dev", name] | ["link", "dev", name] => show_links(name, false)
    ["link", "set", "dev", name, state_word] => link_set(name, state_word)
    ["route"] | ["route", "show"] => show_routes()
    ["route", "add", ..words] => route_change(true, words)
    ["route", "del", ..words] => route_change(false, words)
    else => Err(AppletError.Usage(USAGE))
  }
}

type IpOptions = {operands: List[Str]}

proc main(...argv: List[Str]) [process, env, net, error] {
  let opts: IpOptions = cli.applet(argv, {operands: {form: "...ARG"}})?
  match dispatch(opts.operands) {
    Ok(_) => {}
    Err(AppletError.Usage {message}) => return Err(AppletError.Usage(message))
    Err(net_state.ChangeFailure.Refused {message, ..}) => {
      eprint f"RTNETLINK answers: {message}"
      exit 2
    }
    Err(failure) => {
      eprint f"ip: {failure.message}"
      exit 1
    }
  }
}
