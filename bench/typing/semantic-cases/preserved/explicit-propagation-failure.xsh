proc parsed(value: Str) [error] -> Int { value.parse_int()? }
print ${parsed("bad")}
print unreachable
