type Entry = {name: Str}

let sample_entries = [{name: "file-a.txt"}, {name: "_hidden.txt"}, {name: "file-b.txt"}]

proc run_entries(entries: List[Entry], dry_run: Bool) [error] {
  var kept = 0
  var dropped = 0

  for entry in entries {
    if entry.name.starts_with("_") {
      let label = if dry_run { "would drop" } else { "drop" }
      print f"${label}: ${entry.name}"
      dropped += 1
    } else {
      let label = if dry_run { "would keep" } else { "keep" }
      print f"${label}: ${entry.name}"
      kept += 1
    }
  }

  if dry_run {
    print f"${kept} kept  ${dropped} dropped (dry run)"
  } else {
    print f"${kept} kept  ${dropped} dropped"
  }
}

run_entries(sample_entries, true)?
print "---"
run_entries(sample_entries, false)?
