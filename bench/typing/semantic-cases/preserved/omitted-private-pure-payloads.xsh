pure boolean() { false }
pure early() { return false }
pure optional(flag: Bool) { if flag { 7 } else { null } }
pure result() { Ok(9) }
print ${boolean()} ${early()} ${optional(false) ?? 0} ${result()?}
