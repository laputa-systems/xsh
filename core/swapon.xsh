#!/bin/xsh
use lib.storage

proc main(...argv: List[Str]) {
  storage.dispatch("swapon", argv)
}
