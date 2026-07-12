#!/bin/xsh
proc main(...argv: List[Str]) [time, error] {
  var utc = false
  var format = "%a %b %d %H:%M:%S %Y"

  for arg in argv {
    if arg == "-u" {
      utc = true
    } else if arg.starts_with("+") {
      format = arg.replace("+", "")
    }
  }

  print (time.format(time.now(), format, utc: utc)?)
}
