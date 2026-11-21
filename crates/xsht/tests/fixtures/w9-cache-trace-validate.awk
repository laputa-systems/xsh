function fail(message) {
  print "invalid W9 cache trace: " message > "/dev/stderr"
  failed = 1
}

function full_record_key(module, source, start, len, role, args) {
  return module SUBSEP source SUBSEP start SUBSEP len SUBSEP role SUBSEP args
}

BEGIN {
  FS = "\t"
}

{
  if (NF != 13 || $1 != "W9CACHE" || $2 != "2") {
    fail("malformed or unsupported row " NR)
    next
  }
  if ($3 !~ /^[0-9]+$/ || $4 !~ /^[0-9]+$/) {
    fail("invalid process or event sequence in row " NR)
    next
  }
  if (has_sequence && ($4 + 0) <= previous_sequence) fail("event sequence is not increasing")
  previous_sequence = $4 + 0
  has_sequence = 1
  if ($5 !~ /^[0-9]+$/) {
    fail("invalid cache owner in row " NR)
    next
  }
  if ($6 != "shared" && $6 != "private") {
    fail("unknown cache kind " $6)
    next
  }
  if (owner_kind[$5] && owner_kind[$5] != $6) fail("owner kind changed")
  owner_kind[$5] = $6

  if ($7 == "cache") {
    owner_rows[$5]++
    if ($8 != "-" || $9 != "-" || $10 != "-" || $11 != "-" || $12 != "-" || $13 != "-") {
      fail("cache declaration has key fields in row " NR)
    }
    next
  }
  if (owner_rows[$5] != 1) fail("record event precedes or lacks its cache declaration")
  if ($7 != "requested" && $7 != "prepared" && $7 != "reuse" && $7 != "checked" && $7 != "check_hit") {
    fail("unknown event " $7)
    next
  }
  if ($8 == "" || $9 == "" || $10 !~ /^[0-9]+$/ || $11 !~ /^[0-9]+$/ ||
      ($12 != "Entry" && $12 != "User" && $12 != "Embedded") ||
      ($13 != "Str" && $13 != "Bytes")) {
    fail("invalid module record key in row " NR)
    next
  }

  key = full_record_key($8, $9, $10, $11, $12, $13)
  owner_key = $5 SUBSEP key
  event_keys[owner_key] = 1
  if ($7 == "requested") {
    requested[owner_key]++
    requests_by_kind[$6]++
    if ($12 == "Entry") entry_requests++
    if ($12 != "Entry") {
      if ($6 == "shared") shared_module_owners[key SUBSEP $5] = 1
      else private_module_owners[key SUBSEP $5] = 1
    }
  } else if ($7 == "prepared") {
    if (lookup_events[owner_key] >= requested[owner_key]) {
      fail("cache lookup event precedes a request")
    }
    lookup_events[owner_key]++
    prepared[owner_key]++
    if (prepared[owner_key] > 1) fail("duplicate prepared record for one cache key")
    prepared_by_kind[$6]++
  } else if ($7 == "reuse") {
    if (lookup_events[owner_key] >= requested[owner_key]) {
      fail("cache lookup event precedes a request")
    }
    lookup_events[owner_key]++
    reused[owner_key]++
    reused_by_kind[$6]++
  } else if ($7 == "checked") {
    if (check_events[owner_key] >= requested[owner_key]) {
      fail("check event precedes a request")
    }
    check_events[owner_key]++
    checked[owner_key]++
    if (checked[owner_key] > 1) fail("duplicate check for one cache key")
    checked_by_kind[$6]++
  } else {
    if (check_events[owner_key] >= requested[owner_key]) {
      fail("check event precedes a request")
    }
    if (checked[owner_key] == 0) fail("check hit precedes completed check")
    if (check_hits[owner_key] >= reused[owner_key]) {
      fail("check hit precedes cache reuse")
    }
    check_events[owner_key]++
    check_hits[owner_key]++
    check_hits_by_kind[$6]++
  }
}

END {
  if (NR == 0) fail("empty trace")
  if (entry_requests == 0) fail("trace contains no requested entry record")

  for (owner in owner_kind) {
    if (owner_rows[owner] != 1) fail("cache owner must have one declaration row")
    owner_counts[owner_kind[owner]]++
  }
  for (owner_key in event_keys) {
    if (requested[owner_key] == 0) {
      fail("cache event has no matching requested key")
      continue
    }
    if (prepared[owner_key] != 1) fail("requested key must be prepared once in its owner")
    if (checked[owner_key] != 1) fail("requested key must be checked once in its owner")
    if (prepared[owner_key] + reused[owner_key] != requested[owner_key]) {
      fail("each requested key must have one prepared or reuse event")
    }
    if (checked[owner_key] + check_hits[owner_key] != requested[owner_key]) {
      fail("each requested key must have one checked or check_hit event")
    }
    if (reused[owner_key] != check_hits[owner_key]) {
      fail("cache reuse and check-hit counts differ for one key")
    }
  }

  for (identity in shared_module_owners) {
    split(identity, parts, SUBSEP)
    key = parts[1] SUBSEP parts[2] SUBSEP parts[3] SUBSEP parts[4] SUBSEP parts[5] SUBSEP parts[6]
    shared_owner_count[key]++
  }
  for (key in shared_owner_count) {
    if (shared_owner_count[key] > 1) {
      shared_repeated_keys++
      fail("module key is requested by multiple shared cache owners")
    }
  }

  for (identity in private_module_owners) {
    split(identity, parts, SUBSEP)
    key = parts[1] SUBSEP parts[2] SUBSEP parts[3] SUBSEP parts[4] SUBSEP parts[5] SUBSEP parts[6]
    private_owner_count[key]++
  }
  for (key in private_owner_count) {
    if (private_owner_count[key] > 1) private_repeated_keys++
  }

  for (i = 1; i <= 2; i++) {
    kind = i == 1 ? "shared" : "private"
    if (owner_counts[kind] > 0) {
      print kind " caches=" owner_counts[kind] \
        " requests=" requests_by_kind[kind] + 0 \
        " records=" prepared_by_kind[kind] + 0 \
        " cache_reuses=" reused_by_kind[kind] + 0 \
        " checks=" checked_by_kind[kind] + 0 \
        " check_hits=" check_hits_by_kind[kind] + 0
    }
  }
  print "shared module keys across multiple owners=" shared_repeated_keys + 0
  print "private repeated module keys across owners=" private_repeated_keys + 0
  if (failed) exit 1
}
