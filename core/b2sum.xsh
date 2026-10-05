#!/bin/xsh
use lib.checksums

proc main(...argv: List[Str]) [fs, io, error, process, env] {
  checksums.main(argv, "blake2b")
}
