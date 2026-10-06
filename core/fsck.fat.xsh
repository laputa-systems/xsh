#!/bin/xsh
use lib.fat as dos

proc main(...argv: List[Str]) { dos.fsck(argv) }
