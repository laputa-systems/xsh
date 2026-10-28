use core.lib.dhcp_machine as machine
use core.lib.dhcp_net as network
use core.lib.dhcp_wire as wire
use core.tests.support.dhcp_kernel as kernel_model

const IFINDEX = 3
const MAC = b"\x02\x00\x00\x00\x00\x01"
const ADDRESS = 3232235876
const BROADCAST = 3232236031
const ROUTER = 3232235777
const NEW_ADDRESS = 3232235877
const NEW_ROUTER = 3232235778
const FOREIGN = 167903489
const METRIC = 100

pure lease(address: Int, router: Int) -> machine.Lease {
  machine.Lease(
    address:,
    prefix: 24,
    broadcast: BROADCAST,
    routers: [router],
    dns: [ROUTER],
    domain: null,
    server: ROUTER,
    lease_secs: 600,
    acquired_at: 0,
    renew_at: 300000,
    rebind_at: 525000,
    expire_at: 600000,
    timers: .Default,
  )
}

pure host(address: Int) -> network.Address {
  {address: address, prefix: 24, broadcast: BROADCAST}
}

pure default_via(router: Int) -> network.Route {
  {destination: 0, prefix: 0, gateway: router, table: 254, tos: 0, priority: METRIC, protocol: 16}
}

pure connected(address: Int) -> network.Route {
  {destination: address - address % 256, prefix: 24, gateway: 0, table: 254, tos: 0, priority: 0, protocol: 2}
}

pure link(addresses: List[network.Address], routes: List[network.Route], up: Bool) -> network.Observation {
  {ifindex: IFINDEX, name: "eth0", mac: MAC, up: up, carrier: true, addresses: addresses, routes: routes}
}

pure static_route() -> network.Route {
  {destination: 167837696, prefix: 16, gateway: ROUTER, table: 254, tos: 0, priority: 0, protocol: 4}
}

pure owned_of(addresses: List[network.Address], routes: List[network.Route]) -> network.Owned {
  {ifindex: IFINDEX, addresses: addresses, routes: routes}
}

pure op_names(operations: List[network.Operation]) -> List[Str] {
  [kernel_model.operation_name(operation) for operation in operations]
}

test test_a_fresh_lease_adds_the_address_then_the_default_route {
  let planned = network.plan(lease(ADDRESS, ROUTER), METRIC, network.empty_owned(IFINDEX), link([], [], true))
  assert op_names(planned.operations) == ["+addr 192.168.1.100", "+route 192.168.1.1"]
  assert planned.owned_after.addresses == [host(ADDRESS)] and planned.owned_after.routes == [default_via(ROUTER)]
  assert planned.conflicts.is_empty()
  let down = network.plan(lease(ADDRESS, ROUTER), METRIC, network.empty_owned(IFINDEX), link([], [], false))
  assert op_names(down.operations)[0] == "up"
}

test test_a_converged_interface_plans_nothing {
  let owned = owned_of([host(ADDRESS)], [default_via(ROUTER)])
  let observed = link([host(ADDRESS)], [connected(ADDRESS), default_via(ROUTER)], true)
  let planned = network.plan(lease(ADDRESS, ROUTER), METRIC, owned, observed)
  assert planned.operations.is_empty() and planned.conflicts.is_empty() and planned.owned_after == owned
}

test test_a_changed_lease_removes_old_routes_and_addresses_before_adding_new_ones {
  let owned = owned_of([host(ADDRESS)], [default_via(ROUTER)])
  let observed = link([host(ADDRESS)], [connected(ADDRESS), default_via(ROUTER)], true)
  let planned = network.plan(lease(NEW_ADDRESS, NEW_ROUTER), METRIC, owned, observed)
  assert op_names(planned.operations) == ["-route 192.168.1.1", "-addr 192.168.1.100", "+addr 192.168.1.101", "+route 192.168.1.2"]
  assert planned.owned_after.addresses == [host(NEW_ADDRESS)] and planned.owned_after.routes == [default_via(NEW_ROUTER)]
}

test test_withdrawal_removes_only_owned_state_and_leaves_foreign_state { 
  let foreign_address = {address: FOREIGN, prefix: 24, broadcast: FOREIGN + 255 - FOREIGN % 256}
  let owned = owned_of([host(ADDRESS)], [default_via(ROUTER)])
  let observed = link([foreign_address, host(ADDRESS)], [connected(ADDRESS), static_route(), default_via(ROUTER)], true)
  let planned = network.plan(null, METRIC, owned, observed)
  assert op_names(planned.operations) == ["-route 192.168.1.1", "-addr 192.168.1.100"]
  assert planned.owned_after == network.empty_owned(IFINDEX)
  for operation in planned.operations {
    if let .RemoveAddress(item) = operation { assert item != foreign_address }
    if let .RemoveRoute(item) = operation { assert item != static_route() }
  }
}

test test_owned_state_that_is_already_gone_is_forgotten_without_an_operation {
  let owned = owned_of([host(ADDRESS)], [default_via(ROUTER)])
  let planned = network.plan(null, METRIC, owned, link([], [], true))
  assert planned.operations.is_empty() and planned.owned_after == network.empty_owned(IFINDEX)
}

test test_foreign_state_in_the_way_is_reported_and_never_claimed_or_replaced {
  let occupied = link([host(ADDRESS)], [connected(ADDRESS), {...default_via(ROUTER), protocol: 4, gateway: NEW_ROUTER}], true)
  let planned = network.plan(lease(ADDRESS, ROUTER), METRIC, network.empty_owned(IFINDEX), occupied)
  assert planned.operations.is_empty()
  assert [conflict.kind for conflict in planned.conflicts] == ["foreign_address", "foreign_route"]
  assert planned.owned_after == network.empty_owned(IFINDEX)
  let elsewhere = link([{address: ADDRESS, prefix: 16, broadcast: 0}], [], true)
  let moved = network.plan(lease(ADDRESS, ROUTER), METRIC, network.empty_owned(IFINDEX), elsewhere)
  assert [conflict.kind for conflict in moved.conflicts] == ["address_other_prefix"]
}

test test_a_name_is_not_ownership_after_the_interface_is_recreated {
  let stale = {ifindex: 9, addresses: [host(ADDRESS)], routes: [default_via(ROUTER)]}
  let observed = link([host(ADDRESS)], [connected(ADDRESS), default_via(ROUTER)], true)
  let planned = network.plan(lease(ADDRESS, ROUTER), METRIC, stale, observed)
  assert planned.operations.is_empty()
  assert [conflict.kind for conflict in planned.conflicts] == ["foreign_address", "foreign_route"]
  let withdrawn = network.plan(null, METRIC, stale, observed)
  assert withdrawn.operations.is_empty()
}

test test_a_changed_prefix_on_the_same_address_replaces_the_owned_address {
  let owned = owned_of([host(ADDRESS)], [])
  let observed = link([host(ADDRESS)], [connected(ADDRESS)], true)
  let widened = {...lease(ADDRESS, ROUTER), prefix: 16}
  let planned = network.plan(widened, METRIC, owned, observed)
  assert op_names(planned.operations) == ["-addr 192.168.1.100", "+addr 192.168.1.100", "+route 192.168.1.1"]
  assert planned.conflicts.is_empty()
}

test test_a_lease_without_routers_asks_for_no_route {
  let routerless = {...lease(ADDRESS, ROUTER), routers: []}
  let planned = network.plan(routerless, METRIC, network.empty_owned(IFINDEX), link([], [], true))
  assert op_names(planned.operations) == ["+addr 192.168.1.100"]
}

test test_verification_names_each_discrepancy {
  let planned = network.plan(lease(ADDRESS, ROUTER), METRIC, network.empty_owned(IFINDEX), link([], [], false))
  assert network.verify(planned, link([host(ADDRESS)], [default_via(ROUTER)], true)).is_empty()
  let leftovers = network.verify(planned, link([], [], false))
  assert leftovers.len() == 3 and leftovers[0] == "link_down"
  let removal = network.plan(null, METRIC, owned_of([host(ADDRESS)], []), link([host(ADDRESS)], [], true))
  assert network.verify(removal, link([host(ADDRESS)], [], true)) == ["lingering_address 192.168.1.100/24"]
}

pure issue_free(links: List[LinuxNetworkLink], addresses: List[LinuxNetworkAddress], routes: List[LinuxNetworkRoute], state: Str) -> LinuxNetworkDump {
  {addresses: addresses, enumeration_succeeded: state == "complete", issues: [], links: links, routes: routes, rules: [], state: state}
}

pure kernel_link(index: Int, name: Str, flags: Int) -> LinuxNetworkLink {
  {
    address: MAC,
    attributes: [],
    broadcast: null,
    flags: flags,
    hardware_type: 1,
    ifindex: index,
    kind: null,
    lower_ifindex: null,
    master_ifindex: null,
    mtu: 1500,
    name: name,
    name_bytes: bytes.from_text(name),
    operstate: 6,
    rx_bytes: null,
    tx_bytes: null,
  }
}

pure kernel_address(index: Int, address: Str, prefix: Int) -> LinuxNetworkAddress {
  {
    address: address,
    attributes: [],
    broadcast: "192.168.1.255",
    family: "inet",
    flags: 0,
    ifindex: index,
    label: null,
    local: address,
    preferred_lifetime_seconds: null,
    prefix_length: prefix,
    scope: 0,
    valid_lifetime_seconds: null,
  }
}

pure kernel_route(output: Int?, destination: Str?, prefix: Int, gateway: Str?, protocol: Int, priority: Int?, tos: Int) -> LinuxNetworkRoute {
  {
    attributes: [],
    destination: destination,
    destination_prefix_length: prefix,
    family: "inet",
    flags: 0,
    gateway: gateway,
    input_ifindex: null,
    nexthops: [],
    output_ifindex: output,
    preferred_source: null,
    priority: priority,
    protocol: protocol,
    route_type: 1,
    scope: 0,
    source: null,
    source_prefix_length: 0,
    table: 254,
    tos: tos,
  }
}

test test_observation_projects_one_link_and_refuses_untrustworthy_dumps {
  let links = [kernel_link(1, "lo", 9), kernel_link(IFINDEX, "eth0", 65537), kernel_link(4, "wlan0", 1)]
  let addresses = [kernel_address(IFINDEX, "192.168.1.100", 24), kernel_address(4, "10.0.0.2", 24)]
  let routes = [
    kernel_route(IFINDEX, null, 0, "192.168.1.1", 16, 100, 0),
    kernel_route(4, null, 0, "10.0.0.1", 3, 50, 0),
    kernel_route(IFINDEX, "192.168.1.0", 24, null, 2, null, 0),
    kernel_route(IFINDEX, null, 0, "192.168.1.1", 16, 100, 8),
  ]
  let dump = issue_free(links, addresses, routes, "complete")
  let observed = network.observe_link(dump, "eth0")?
  assert observed.ifindex == IFINDEX and observed.up and observed.carrier and observed.mac == MAC
  assert observed.addresses == [host(ADDRESS)]
  assert observed.routes.len() == 3
  assert observed.routes[0] == default_via(ROUTER)
  assert observed.routes[2].tos == 8 and observed.routes[0].tos == 0, "routes that differ only in tos stay distinct"
  assert observed.routes[1].destination == 3232235776 and observed.routes[1].protocol == 2

  assert network.observe_link(issue_free(links, addresses, routes, "truncated"), "eth0") is Err(_)
  assert network.observe_link(issue_free(links, addresses, routes, "interrupted"), "eth0") is Err(_)
  assert network.observe_link(dump, "eth9") is Err(_)
  let twice = issue_free([kernel_link(2, "eth0", 1), kernel_link(5, "eth0", 1)], [], [], "complete")
  assert network.observe_link(twice, "eth0") is Err(_)
  let broken = issue_free(links, [kernel_address(IFINDEX, "not-an-address", 24)], [], "complete")
  assert network.observe_link(broken, "eth0") is Err(_)
}

test test_netlink_requests_encode_the_exact_kernel_messages {
  let add = network.address_request(host(ADDRESS), IFINDEX, true)?
  assert add.message_type == 20 and add.flags == 1 + 4 + 1024 + 512
  assert add.payload == b"\x02\x18\x00\x00\x03\x00\x00\x00\x08\x00\x02\x00\xc0\xa8\x01\x64\x08\x00\x01\x00\xc0\xa8\x01\x64\x08\x00\x04\x00\xc0\xa8\x01\xff"
  let remove = network.address_request(host(ADDRESS), IFINDEX, false)?
  assert remove.message_type == 21 and remove.flags == 1 + 4
  assert remove.payload == b"\x02\x18\x00\x00\x03\x00\x00\x00\x08\x00\x02\x00\xc0\xa8\x01\x64\x08\x00\x01\x00\xc0\xa8\x01\x64"

  let route_add = network.route_request(default_via(ROUTER), IFINDEX, true)?
  assert route_add.message_type == 24 and route_add.flags == 1 + 4 + 1024 + 512
  assert route_add.payload == b"\x02\x00\x00\x00\xfe\x10\x00\x01\x00\x00\x00\x00\x08\x00\x05\x00\xc0\xa8\x01\x01\x08\x00\x04\x00\x03\x00\x00\x00\x08\x00\x06\x00\x64\x00\x00\x00"
  let route_del = network.route_request(default_via(ROUTER), IFINDEX, false)?
  assert route_del.message_type == 25 and route_del.flags == 1 + 4
  assert route_del.payload == b"\x02\x00\x00\x00\xfe\x10\xff\x00\x00\x00\x00\x00\x08\x00\x05\x00\xc0\xa8\x01\x01\x08\x00\x04\x00\x03\x00\x00\x00\x08\x00\x06\x00\x64\x00\x00\x00"
  let with_destination = network.route_request({destination: 167837696, prefix: 16, gateway: ROUTER, table: 254, tos: 8, priority: 7, protocol: 16}, IFINDEX, true)?
  assert with_destination.payload[..12] == b"\x02\x10\x00\x08\xfe\x10\x00\x01\x00\x00\x00\x00"
  assert with_destination.payload[12..20] == b"\x08\x00\x01\x00\x0a\x01\x00\x00"
}

test test_a_reconciliation_logs_intent_applies_confirms_and_records_ownership {
  let journal = kernel_model.play(lease(ADDRESS, ROUTER), METRIC, network.empty_owned(IFINDEX), kernel_model.Kernel(observed: link([], [], true), fail_at: -1, calls: 0), false)
  guard let outcome = journal.outcome else {
    assert false, "the reconciliation did not finish"
    return
  }
  assert outcome.settled == .Converged and outcome.conflicts.is_empty()
  assert journal.mutations == ["+addr 192.168.1.100", "+route 192.168.1.1"]
  assert journal.ledgers.len() == 2
  assert journal.ledgers[0].reconcile and journal.ledgers[0].owned.addresses == [host(ADDRESS)]
  assert ! journal.ledgers[1].reconcile and journal.ledgers[1].owned == outcome.owned
  assert outcome.owned.routes == [default_via(ROUTER)]
  assert journal.kernel.observed.addresses == [host(ADDRESS)]
  assert connected(ADDRESS) in journal.kernel.observed.routes, "the kernel connected route appears and is not tracked"
  assert connected(ADDRESS) not in outcome.owned.routes
}

test test_reapplying_a_converged_interface_mutates_nothing {
  let kernel = kernel_model.Kernel(observed: link([host(ADDRESS)], [connected(ADDRESS), default_via(ROUTER)], true), fail_at: -1, calls: 0)
  let journal = kernel_model.play(lease(ADDRESS, ROUTER), METRIC, owned_of([host(ADDRESS)], [default_via(ROUTER)]), kernel, false)
  assert journal.mutations.is_empty() and journal.ledgers.is_empty()
  assert journal.outcome?.settled == .Converged
}

test test_a_failed_operation_leaves_the_intent_recorded_and_the_next_run_converges {
  let start = kernel_model.Kernel(observed: link([], [], true), fail_at: 1, calls: 0)
  let first = kernel_model.play(lease(ADDRESS, ROUTER), METRIC, network.empty_owned(IFINDEX), start, false)
  guard let partial = first.outcome else {
    assert false, "the reconciliation did not finish"
    return
  }
  if let .Partial(reason) = partial.settled {
    assert "+route" in reason, reason
  } else {
    assert false, "a failed operation must end partial"
  }
  assert first.mutations == ["+addr 192.168.1.100"]
  assert first.ledgers.len() == 1 and first.ledgers[0].reconcile
  assert first.ledgers[0].owned.routes == [default_via(ROUTER)], "the intent names the route that was never added"
  assert partial.owned == first.ledgers[0].owned

  let resumed = kernel_model.play(lease(ADDRESS, ROUTER), METRIC, partial.owned, {...first.kernel, fail_at: -1}, false)
  assert resumed.mutations == ["+route 192.168.1.1"], "only the missing operation is repeated"
  assert resumed.outcome?.settled == .Converged
  assert resumed.kernel.observed.addresses == [host(ADDRESS)]
}

test test_a_stale_observation_replans_from_the_fresh_one {
  let kernel = kernel_model.Kernel(observed: link([], [], true), fail_at: -1, calls: 0)
  let journal = kernel_model.play(lease(ADDRESS, ROUTER), METRIC, network.empty_owned(IFINDEX), kernel, true)
  guard let outcome = journal.outcome else {
    assert false, "the reconciliation did not finish"
    return
  }
  assert outcome.settled == .Converged
  assert host(FOREIGN) in journal.kernel.observed.addresses, "the address that appeared meanwhile survives"
  assert host(FOREIGN) not in outcome.owned.addresses
  assert journal.mutations == ["+addr 192.168.1.100", "+route 192.168.1.1"]
}

test test_withdrawal_after_nak_keeps_foreign_state_through_the_whole_walk {
  let foreign = {address: FOREIGN, prefix: 24, broadcast: 0}
  let kernel = kernel_model.Kernel(observed: link([foreign, host(ADDRESS)], [connected(ADDRESS), static_route(), default_via(ROUTER)], true), fail_at: -1, calls: 0)
  let journal = kernel_model.play(null, METRIC, owned_of([host(ADDRESS)], [default_via(ROUTER)]), kernel, false)
  assert journal.mutations == ["-route 192.168.1.1", "-addr 192.168.1.100"]
  assert journal.kernel.observed.addresses == [foreign]
  assert static_route() in journal.kernel.observed.routes
  assert journal.outcome?.owned == network.empty_owned(IFINDEX)
}

test test_conflicts_end_the_walk_without_touching_the_occupant {
  let occupied = kernel_model.Kernel(observed: link([host(ADDRESS)], [connected(ADDRESS)], true), fail_at: -1, calls: 0)
  let journal = kernel_model.play(lease(ADDRESS, ROUTER), METRIC, network.empty_owned(IFINDEX), occupied, false)
  assert journal.outcome?.settled == .Conflicted
  assert journal.mutations.len() == 1 and journal.mutations[0] == "+route 192.168.1.1"
}

test test_feedback_that_does_not_fit_the_phase_ends_the_walk_as_partial {
  let started = network.begin_walk(lease(ADDRESS, ROUTER), METRIC, network.empty_owned(IFINDEX))
  let wrong = network.advance(started.walk, network.Mutated)
  if let .Finished(outcome) = wrong.action {
    match outcome.settled {
      network.Partial(_) => {}
      else => assert false, "unexpected feedback must be partial"
    }
  } else {
    assert false, "unexpected feedback must finish the walk"
  }
  let finished = network.advance(wrong.walk, network.Persisted)
  if let .Finished(_) = finished.action {} else { assert false, "a closed walk stays closed" }
}
