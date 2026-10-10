#!/bin/xsh
use lib.tracepath

proc main(...argv: List[Str]) {
  tracepath.execute(argv)
}
