# Diff two dhat summaries produced by tools/dhat-summarize.xsh (before vs after).
# Allocation bytes are deterministic for a fixed corpus + allocator, so a drop in
# total/site bytes is a real win. Used to validate optimization commits (e.g.
# 816ea0a's btreemap reduction).
#
# Usage: xsh perf/dhat-compare.xsh -- <before.summary.json> <after.summary.json>
error CompareError = Failed(message: Str)

type DhatSite = {frames: List[Str], total_bytes: Int, max_bytes: Int, total_blocks: Int}

type DhatSummary = {
  version: Int,
  scenario: Str,
  total_bytes: Int,
  total_blocks: Int,
  gmax_bytes: Int,
  sites: List[DhatSite],
}

type SiteDelta = {key: Str, before: Int, after: Int, delta: Int}

pure site_key(site: DhatSite) -> Str {
  return site.frames.join(" <- ")
}

pure find_site(sites: List[DhatSite], key: Str) -> Result[DhatSite] {
  for site in sites {
    if site_key(site) == key {
      return site
    }
  }

  return Err(CompareError.Failed(f"missing site: ${key}"))
}

proc report_total(name: Str, before: Int, after: Int) [io] {
  let delta = after - before
  let adelta = if delta < 0 { -delta } else { delta }
  let dsign = if delta > 0 { "+" } else if delta < 0 { "-" } else { "" }
  let pm = if before == 0 { 0 } else { adelta * 1000 / before }
  print f"  ${name}: ${before} -> ${after} (${dsign}${adelta}, ${dsign}${pm / 10}.${pm % 10}%)"
}

let script_args = args

if script_args.len() < 2 {
  eprint "usage: dhat-compare.xsh -- <before.summary.json> <after.summary.json>"
  abort(2)
}

let before = json.read(fp"${script_args[0]}")?.require(DhatSummary)?
let after = json.read(fp"${script_args[1]}")?.require(DhatSummary)?

print f"dhat before/after: ${before.scenario} -> ${after.scenario}"
report_total("total_bytes", before.total_bytes, after.total_bytes)
report_total("total_blocks", before.total_blocks, after.total_blocks)
report_total("gmax_bytes", before.gmax_bytes, after.gmax_bytes)
print ""
print "  largest per-site byte changes:"

var rows: List[SiteDelta] = []

for b_site in before.sites {
  let key = site_key(b_site)

  let after_bytes = match find_site(after.sites, key) {
    Ok(site) => site.total_bytes
    Err(_) => 0
  }

  rows = rows.push({key, before: b_site.total_bytes, after: after_bytes, delta: after_bytes - b_site.total_bytes})
}

for a_site in after.sites {
  let key = site_key(a_site)

  match find_site(before.sites, key) {
    Ok(_) => {}
    Err(_) => rows = rows.push({key, before: 0, after: a_site.total_bytes, delta: a_site.total_bytes})
  }
}

let ranked = rows
  |> sort-by .delta
  |> where .delta != 0

for row in ranked |> take(20) {
  let sign = if row.delta > 0 { "+" } else { "" }
  let leaf = row.key.split(" <- ")[0]
  print f"  ${sign}${row.delta}  ${row.before}->${row.after}  ${leaf}"
}

let total_delta = after.total_bytes - before.total_bytes

if total_delta > 0 {
  print ""
  print f"total allocation bytes increased by ${total_delta} (after is worse)"
  abort(1)
}
