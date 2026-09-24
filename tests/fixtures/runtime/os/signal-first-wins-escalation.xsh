let ready = fp"${ARGV[0]}"
let hook_entered = fp"${ARGV[1]}"

on USR1 [fs, time, error] {
  hook_entered.write("entered")?
  while true {
    time.sleep(10ms)?
  }
}

on USR2 [] {
  print "wrong-hook"
  abort(2)
}

ready.write("ready")?
while true {
  time.sleep(10ms)?
}
