let assignment: Regex = rx"^\s*[A-Z_]+=([0-9]+)$"
let matched = assignment.matches("COUNT=42")
let number = assignment.captures("COUNT=42")[1]
let runtime_pattern = "[a-z]+"
let dynamic: Regex = regex.compile(runtime_pattern)?
