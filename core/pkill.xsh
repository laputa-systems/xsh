#!/bin/xsh
use lib.procps
proc main(...argv: List[Str]) [fs, process, env, error, io] { procps.grep_main(argv, true) }
