const state = p"deploy.state"

on SIGTERM [fs, error] {
  state.write("interrupted\n")
  exit 143
}

state.write("deploying\n")
run sleep 30
state.write("done\n")
