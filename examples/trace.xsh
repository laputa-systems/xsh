proc say(message: Str) {
  print $message
}

say("traced")
let done = run.text printf "%s\n" "done" ?
print done.trim()
