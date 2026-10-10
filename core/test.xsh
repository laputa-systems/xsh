#!/bin/xsh
use lib.testexpr

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  testexpr.evaluate(argv)
}
