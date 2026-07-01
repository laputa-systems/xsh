let ready = fp"${ARGV[0]}"
let leaked = fp"${ARGV[1]}"
let output = fp"${ARGV[2]}"
let helper = fp"${ARGV[3]}"
run ${helper} group-leak ${ready} ${leaked} | run cat > output ?
