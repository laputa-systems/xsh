let root = p"target/modules-basic-fixture"
fs.remove $root --missing-ok ?
fs.mkdir $root ?
let file = fp"${root}/note.txt"
fs.write $file " hello\nworld\n" ?
let file_bytes = file.read_bytes()?
let exists = file.exists()?
let normalized = fp"${root}/./child/../note.txt".normalize()
let display = normalized.display()

let lines = """ a
b
""".trim().lines().collect()

let has_name = display.ends_with("note.txt")
let has_root = "modules-basic-fixture" in display
let has_path_root = "modules-basic-fixture" in normalized
let starts = "  stage".trim().starts_with("stage")
let contains = display.contains("note")
let enough_cpu = cpu.count() > 0
let parsed = "42".parse_int()?
let tokens = cli.tokens(["-dc", "--wrap=0", "-1", "file"], ["wrap"])?
let token_str = f"${tokens[0].name}${tokens[1].name}${tokens[2].name}:${tokens[2].value}:${tokens[3].kind}:${tokens[3].name}"
print $exists ${file_bytes == b" hello\nworld\n"} lines[1] $has_name $has_root $has_path_root $starts $contains $enough_cpu $parsed $token_str
