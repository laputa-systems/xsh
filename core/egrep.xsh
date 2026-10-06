#!/bin/xsh
use lib.search

proc main(...args: List[Str]) {
  search.grep(args, "extended")
}
