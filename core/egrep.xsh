#!/bin/xsh
use lib.gnu_grep

proc main(...args: List[Bytes]) {
  gnu_grep.grep_main(args, "E", "egrep")
}
