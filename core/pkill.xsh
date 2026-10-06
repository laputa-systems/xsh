#!/bin/xsh
use lib.procps
proc main(...argv: List[Str]) [process, env, error, io] { procps.grep_main(argv, true) }
