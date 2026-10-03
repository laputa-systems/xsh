# Script arguments use the single predeclared binding; byte lengths are explicit.
let argument_count = args.len()
let byte_count = "\u{e9}\u{1f343}".byte_len()
let character_count = "\u{e9}\u{1f343}".count_chars()
print ${argument_count} ${byte_count} ${character_count}
