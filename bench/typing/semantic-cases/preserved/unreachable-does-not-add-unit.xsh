pure early() { return 7; return "unreachable" }
pure branch(flag: Bool) -> Int { if flag { return 8 } else { 9 } }
print ${early()} ${branch(true)} ${branch(false)}
