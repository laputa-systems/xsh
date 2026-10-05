e"STAGE" = "build" # set for the rest of the script

env LC_ALL=C {
  e"STAGE" = "test" # undone when this scope ends
  e"RETRIES" = 3 # converted like an argv item
  let stage = run.text printenv STAGE
  let retries = run.text printenv RETRIES
  print f"child sees: {stage.trim()} {retries.trim()}"
}

print f"after the scope: STAGE={e"STAGE"?} RETRIES={e"RETRIES" ?? "(unset)"}"

# `NAME=value` before a command sets it for that one child.
let once = run.text STAGE=deploy printenv STAGE
print f"one command saw {once.trim()}; the script still has {e"STAGE"?}"

let scratch = fs.tempdir()?
defer scratch.close()
let tools = scratch.host_path()?
let tool = fp"{tools}/hello-tool"
tool.write("#!/bin/sh\necho \"hello from $STAGE\"\n", mode: 0o755)

env STAGE=release {
  env.PATH.prepend(tools)
  run hello-tool
}

print f"tools on PATH afterwards: {tools in env.PATH}"
