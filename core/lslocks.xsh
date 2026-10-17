#!/bin/xsh
use lib.locks

proc main(...argv: List[Str]) {
  if process.signal_action("PIPE")? == "default" {
    process.set_signal_action("PIPE", "default")?
  }
  locks.main(@argv)
}
