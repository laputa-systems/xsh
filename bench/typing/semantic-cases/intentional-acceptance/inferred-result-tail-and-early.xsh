pure tail(value) { value.parse_int()? }
pure early(value) { let parsed = value.parse_int()?; return parsed }
print ${tail("7")?} ${early("8")?}
print ${tail("bad") is Err(_)}
