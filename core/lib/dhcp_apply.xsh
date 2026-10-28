##! The effectful side of lease configuration: observing one link through
##! route netlink, applying the operations of a plan, and driving a pure
##! reconciliation to its end while persisting the owned record around every
##! change. All decisions stay in `dhcp_net`; this module only performs the
##! actions it returns and reports what happened.
use dhcp_machine as machine
use dhcp_net as network
use dhcp_store as store

## Reads the link `name` and its IPv4 state from one complete dump. An
## interrupted or truncated dump is an error, never an empty interface.
export proc observe(name: Str) [process, net, error] -> Result[network.Observation, Error] {
  let dump = linux.network_dump()?
  network.observe_link(dump, name)?
}

proc send(request: network.Request) [process, net, error] {
  let c = linux.net_constants()
  let channel = linux.netlink_open(c.NETLINK_ROUTE)?
  defer unix.close_fd(channel)
  let _ = linux.netlink_request(channel, request.message_type, request.flags, request.payload)?
  Ok()
}

## Performs one planned operation on the link `name` with index `ifindex`.
export proc mutate(name: Str, ifindex: Int, operation: network.Operation) [process, net, error] -> Result[Unit, Error] {
  match operation {
    network.BringUp => linux.link_up(name)
    network.AddAddress(item) => send(network.address_request(item, ifindex, true)?)
    network.RemoveAddress(item) => send(network.address_request(item, ifindex, false)?)
    network.AddRoute(item) => send(network.route_request(item, ifindex, true)?)
    network.RemoveRoute(item) => send(network.route_request(item, ifindex, false)?)
  }
}

proc persist(root: FsRoot, base: store.LeaseRecord, owned: network.Owned, flagged: Bool) [fs, error] {
  store.save(root, {...base, owned: owned, reconcile: flagged})
}

proc step(root: FsRoot, base: store.LeaseRecord, action: network.Action) [fs, process, net, error] -> Result[network.Feedback, Error] {
  match action {
    network.DoObserve => {
      match observe(base.name) {
        Ok(observed) => Ok(.Observed(observed))
        Err(failure) => Err(failure)
      }
    }
    network.DoPersist(ledger) => {
      persist(root, base, ledger.owned, ledger.reconcile)
      Ok(network.Persisted)
    }
    network.DoMutate(operation) => {
      match mutate(base.name, base.ifindex, operation) {
        Ok(_) => Ok(network.Mutated)
        Err(failure) => Ok(.MutationFailed(failure.message))
      }
    }
    network.Finished(_) => Err(error.failure("a finished reconciliation has no next step"))
  }
}

## Reconciles the link of `base` toward the lease `desired` (null withdraws
## everything the record owns). The record is rewritten under `root` with the
## intended ownership before any change, and with the final ownership once a
## fresh observation confirms the result. A failed operation ends the run as
## `Partial` with the intent still on disk; the next run re-plans from
## observation. A failed observation or write is an error.
export proc reconcile(root: FsRoot, base: store.LeaseRecord, desired: machine.Lease?, metric: Int) [fs, process, net, error] -> Result[network.Outcome, Error] {
  var move = network.begin_walk(desired, metric, base.owned)
  while true {
    if let .Finished(outcome) = move.action {
      return Ok(outcome)
    }
    let feedback = step(root, base, move.action)?
    move = network.advance(move.walk, feedback)
  }
  Err(error.failure("reconciliation loop exited without finishing"))
}
