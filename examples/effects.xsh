pure format_label(kind: Str, value: Str) -> Str {
  return f"${kind}: ${value}"
}

proc epoch_year() [time, error] -> Result[Str] {
  return format_label("year", time.format(0, "%Y", utc: true)?)
}

let label = epoch_year()?
print $label
