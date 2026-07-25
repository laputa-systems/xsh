let output = fp"${args[0]}"
run printf "gamma\nalpha\nbeta\n" | run sort > ${output}
