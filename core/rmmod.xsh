#!/bin/xsh
use lib.kmod

proc main(...argv: List[Str]) {
  kmod.rmmod(argv)
}
