pure meminfo_issue(field) -> Str {
  f"meminfo.{field}"
}

print ${meminfo_issue("MemTotal")}
