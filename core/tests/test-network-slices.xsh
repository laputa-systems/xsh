use core.lib.icmp
use core.lib.net_sockets as sockets

test test_mapped_ipv6_text_clamps_a_truncated_ipv4_tail {
  let tail_12 = bytes.concat([bytes.zero(10)?, b"\xff\xff"])
  let tail_13 = bytes.concat([tail_12, b"\x7f"])
  let tail_14 = bytes.concat([tail_13, b"\x00"])
  let tail_15 = bytes.concat([tail_14, b"\x00"])
  let tail_16 = bytes.concat([tail_15, b"\x01"])

  assert icmp.ipv6_text(tail_12) == "::ffff:0.0.0.0"
  assert icmp.ipv6_text(tail_13) == "::ffff:127.0.0.0"
  assert icmp.ipv6_text(tail_14) == "::ffff:127.0.0.0"
  assert icmp.ipv6_text(tail_15) == "::ffff:127.0.0.0"
  assert icmp.ipv6_text(tail_16) == "::ffff:127.0.0.1"

  assert sockets.ipv6_text(tail_12) == "::ffff:0.0.0.0"
  assert sockets.ipv6_text(tail_13) == "::ffff:127.0.0.0"
  assert sockets.ipv6_text(tail_14) == "::ffff:127.0.0.0"
  assert sockets.ipv6_text(tail_15) == "::ffff:127.0.0.0"
  assert sockets.ipv6_text(tail_16) == "::ffff:127.0.0.1"
}
