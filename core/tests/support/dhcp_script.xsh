##! Test support: constructors for the scripted server's rules and replies, so a
##! test names only what differs from a well-behaved server.
use tests.support.dhcp_types as types

## A well-behaved reply of `kind` ("offer", "ack" or "nak") for the test
## subnet: address 192.168.50.100/24, a 600 second lease, the server as router
## and resolver, the request's transaction, broadcast delivery.
export pure reply(kind: Str) -> types.Reply {
  types.Reply(
    kind:,
    address: "192.168.50.100",
    lease: 600,
    t1: null,
    t2: null,
    mask: "255.255.255.0",
    routers: ["192.168.50.1"],
    dns: ["192.168.50.1"],
    domain: null,
    server_id: "192.168.50.1",
    xid: "same",
    chaddr: "same",
    dest: "broadcast",
    corrupt: "none",
    repeat: 1,
    delay_ms: 0,
  )
}

## Answers every request of kind `on` with `replies`.
export pure always(on: Str, replies: List[types.Reply]) -> types.Rule {
  types.Rule(on:, nth: null, from: null, replies:)
}

## Answers only the `nth` request of kind `on`.
export pure only(on: Str, nth: Int, replies: List[types.Reply]) -> types.Rule {
  types.Rule(on:, nth:, from: null, replies:)
}

## Answers the requests of kind `on` from the `first` onward.
export pure from(on: Str, first: Int, replies: List[types.Reply]) -> types.Rule {
  types.Rule(on:, nth: null, from: first, replies:)
}
