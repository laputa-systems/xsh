# Script arguments use the single predeclared binding; byte lengths are explicit.
let argument_count = args.len()
let byte_count = "é🍃".byte_len()
let character_count = "é🍃".count_chars()
print ${argument_count} ${byte_count} ${character_count}
