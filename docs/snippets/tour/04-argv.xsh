const file = "quarterly report.pdf"
const flags = ["-l", "-a"]
let argv = run.text printf "<%s>\n" $file @flags "*.log" ?
print argv.trim()
