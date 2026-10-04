#!/bin/xsh
use lib.testexpr

proc main(...argv: List[Str]) [fs, process, env, io, error] {
  testexpr.evaluate(argv)
}
