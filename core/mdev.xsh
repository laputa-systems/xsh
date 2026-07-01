#!/usr/bin/env -S xsh --
proc main(...argv: List[Str]) [process] -> Result[Int] {
  return applet.mdev(argv)
}

main(@args)?
