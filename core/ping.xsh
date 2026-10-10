#!/bin/xsh
use lib.ping

# An interrupt ends the run like iputils: the summary block prints from the
# run's deferred cleanup, and the status says whether any reply arrived.
on INT [env, error] {
  exit (e"XSH_PING_STATUS" ?? "1").parse_int() ?? 1
}

proc main(...argv: List[Str]) {
  ping.execute(argv, "any")
}
