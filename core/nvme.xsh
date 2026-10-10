#!/bin/xsh
use lib.nvme_cli

proc main(...argv: List[Str]) {
  nvme_cli.dispatch(argv, p"/sys", p"/dev")
}
