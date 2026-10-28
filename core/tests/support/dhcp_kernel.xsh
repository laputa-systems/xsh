##! Test support: a model of one interface's IPv4 state that applies the
##! operations of a reconciliation the way the kernel does: an address brings
##! its connected route and takes it away, a route that finds its slot taken
##! is refused. It lets tests drive the pure reconciliation, and the in-process
##! fake world, through failures at chosen operations without a real link.
use lib.dhcp_machine as machine
use lib.dhcp_net as network
use lib.dhcp_wire as wire

## The modelled link, how many operations it has performed and which operation
## index fails (-1 for none).
export type Kernel = {observed: network.Observation, fail_at: Int, calls: Int}

## What a reconciliation did to the model.
export type Played = {
  kernel: Kernel,
  outcome: network.Outcome?,
  ledgers: List[network.Ledger],
  mutations: List[Str],
}

## A name for an operation, in the form the tests compare.
export pure operation_name(operation: network.Operation) -> Str {
  match operation {
    network.BringUp => "up"
    network.AddAddress(item) => f"+addr {wire.ip_text(item.address)}"
    network.RemoveAddress(item) => f"-addr {wire.ip_text(item.address)}"
    network.AddRoute(item) => f"+route {wire.ip_text(item.gateway)}"
    network.RemoveRoute(item) => f"-route {wire.ip_text(item.gateway)}"
  }
}

pure subnet_of(address: network.Address) -> Int {
  let size = 4294967296 - (wire.prefix_mask(address.prefix) ?? 4294967040)
  address.address - address.address % size
}

pure connected(address: network.Address) -> network.Route {
  {destination: subnet_of(address), prefix: address.prefix, gateway: 0, table: 254, tos: 0, priority: 0, protocol: 2}
}

pure drop_connected(routes: List[network.Route], address: network.Address) -> List[network.Route] {
  [item for item in routes if ! (item.protocol == 2 and item.destination == subnet_of(address) and item.prefix == address.prefix)]
}

## Applies one operation to the model.
export pure simulate(kernel: Kernel, operation: network.Operation) -> Kernel {
  let state = kernel.observed
  let next = match operation {
    network.BringUp => {...state, up: true}
    network.AddAddress(item) => {...state, addresses: state.addresses.push(item), routes: state.routes.push(connected(item))}
    network.RemoveAddress(item) => {...state, addresses: [held for held in state.addresses if held != item], routes: drop_connected(state.routes, item)}
    network.AddRoute(item) => {...state, routes: state.routes.push(item)}
    network.RemoveRoute(item) => {...state, routes: [held for held in state.routes if held != item]}
  }
  {...kernel, observed: next, calls: kernel.calls + 1}
}

## Runs a whole reconciliation toward `desired` against the model, recording
## what was persisted and performed. `drift` adds a foreign address to the
## second observation, the way an address appearing between planning and
## applying would.
export pure play(desired: machine.Lease?, metric: Int, owned: network.Owned, kernel: Kernel, drift: Bool) -> Played {
  var move = network.begin_walk(desired, metric, owned)
  var state = kernel
  var ledgers = []
  var mutations = []
  var observations = 0
  repeat 60 times {
    match move.action {
      network.Finished(outcome) => {
        return {kernel: state, outcome: outcome, ledgers: ledgers, mutations: mutations}
      }
      network.DoObserve => {
        observations += 1
        let seen = if drift and observations == 2 { {...state.observed, addresses: state.observed.addresses.push({address: 167903489, prefix: 24, broadcast: 3232236031})} } else { state.observed }
        state = {...state, observed: seen}
        move = network.advance(move.walk, .Observed(seen))
      }
      network.DoPersist(ledger) => {
        ledgers += [ledger]
        move = network.advance(move.walk, .Persisted)
      }
      network.DoMutate(operation) => {
        if state.fail_at == state.calls {
          move = network.advance(move.walk, .MutationFailed(f"injected failure at {operation_name(operation)}"))
        } else {
          mutations += [operation_name(operation)]
          state = simulate(state, operation)
          move = network.advance(move.walk, .Mutated)
        }
      }
    }
  }
  {kernel: state, outcome: null, ledgers: ledgers, mutations: mutations}
}
