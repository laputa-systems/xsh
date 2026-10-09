#!/bin/xsh
use lib.testexpr

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  testexpr.evaluate(argv, cli.argv_bytes())
}
