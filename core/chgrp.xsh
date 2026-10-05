#!/bin/xsh
use lib.perm
proc main(...argv: List[Str]) [fs, error, process, env, io] { perm.ownership(argv, group_only: true) }
