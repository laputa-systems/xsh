##! Test support: brings the server end of the veth pair up with the server
##! address. Arguments: interface, address, prefix length.
use lib.dhcp_wire as wire
use lib.nettools_fixture as fixture

proc main(...argv: List[Str]) [process, net, error] {
  let prefix = argv[2] as Int
  let mask = wire.prefix_mask(prefix) ?? 0
  let address = wire.ip_parse(argv[1]) ?? 0
  let host = address - address % (4294967296 - mask)
  let broadcast = wire.ip_text(host + (4294967295 - mask))
  fixture.link_up(argv[0])
  fixture.address_add4(argv[0], argv[1], prefix, broadcast, "")
}
