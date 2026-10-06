#!/bin/xsh
use lib.compress

proc main(...argv: List[Str]) [fs, io, error, process, env] {
  compress.execute(argv, "bz2", true, true)
}
