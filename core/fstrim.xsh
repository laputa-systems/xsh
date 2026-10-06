#!/bin/xsh
use lib.storage

proc main(...argv: List[Str]) {
  storage.dispatch("fstrim", argv)
}
