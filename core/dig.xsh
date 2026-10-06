#!/bin/xsh
use lib.net_diagnostics

proc main(...argv: List[Str]) [net, process, env, io, error] {
  net_diagnostics.dig(argv)
}
