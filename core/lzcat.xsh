#!/bin/xsh
use lib.compress

proc main(...argv: List[Str]) [fs, io, error, process, env, time] {
  compress.execute(argv, "lzma", true, true)
}
