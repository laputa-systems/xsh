pure meminfo_issue(field: Str) -> Str {
  f"meminfo.{field}"
}

print ${meminfo_issue("MemTotal")}
