#!/bin/xsh
proc main(...argv: List[Str]) [process] -> Result[Int] {
  applet.mdev(argv)
}

exit main(@args)?
