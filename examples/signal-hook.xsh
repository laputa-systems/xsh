on USR1 [] {
  print "signal"
  abort(0)
}

run sh -c "kill -USR1 \$PPID" ?
