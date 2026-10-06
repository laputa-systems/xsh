#!/bin/xsh
use lib.kmod

proc main(...argv: List[Str]) {
  kmod.depmod(argv)
}
