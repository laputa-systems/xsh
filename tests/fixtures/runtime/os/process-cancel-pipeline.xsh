let ready = fp"${args[0]}"
let leaked = fp"${args[1]}"
let output = fp"${args[2]}"
let helper = fp"${args[3]}"
run ${helper} group-leak ${ready} ${leaked} | run cat > output ?
