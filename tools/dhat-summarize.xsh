# Summarize a dhat-heap.json (produced by `xsh --features dhat-heap`, i.e.
# `make prof-dhat`) into a top-allocation-sites text report plus a normalized
# JSON baseline for before/after diffing (see perf/dhat-compare.xsh).
#
# The interactive view is dhat/dh_view.html; this is the quick top-N + the
# stable, address-free shape the compare script consumes.
#
# Usage: xsh tools/dhat-summarize.xsh -- <dhat-heap.json> <scenario> [out.json]
error SummarizeError = Failed(message: Str)

type DhatPP = {tb: Int, tbk: Int, mb: Int, gb: Int, fs: List[Int]}

type DhatFile = {pps: List[DhatPP], ftbl: List[Str]}

type DhatSite = {frames: List[Str], total_bytes: Int, max_bytes: Int, total_blocks: Int}

type DhatSummary = {
  version: Int,
  scenario: Str,
  total_bytes: Int,
  total_blocks: Int,
  gmax_bytes: Int,
  sites: List[DhatSite],
}

# Strip dhat's volatile "0xADDR: " prefix and trailing " (loc)" so a frame is a
# stable symbol that matches across builds/revisions.
pure clean_frame(raw: Str) -> Str {
  var s = raw.trim()

  if s.starts_with("0x") {
    let parts = s.split(": ")

    if parts.len() >= 2 {
      s = parts[1..].join(": ")
    }
  }

  return s.split(" (")[0].trim()
}

# Pure allocator/collection plumbing frames; skipped when picking the meaningful
# site to display (kept in the JSON stack for matching).
pure is_noise(frame: Str) -> Bool {
  let needles = [
    "alloc::alloc::Global",
    "__rust_alloc",
    "raw_vec::RawVecInner",
    "dhat::Alloc",
    "alloc::boxed::box_new",
    "core::alloc",
  ]

  for needle in needles {
    if frame.contains(needle) {
      return true
    }
  }

  return false
}

pure site_frames(pp: DhatPP, ftbl: List[Str]) -> List[Str] {
  return [clean_frame(ftbl.get(idx, "?")) for idx in pp.fs |> take(8)]
}

pure first_meaningful(frames: List[Str]) -> Str {
  for frame in frames {
    if ! is_noise(frame) {
      return frame
    }
  }

  return frames.get(0, "?")
}

let script_args = args

if script_args.len() < 2 {
  eprint "usage: dhat-summarize.xsh -- <dhat-heap.json> <scenario> [out.json]"
  abort(2)
}

let in_path = fp"${script_args[0]}"
let scenario = script_args[1]
let top = env.int("XSH_DHAT_TOP", 25)?
let doc = json.read(in_path)?.require(DhatFile)?
var total_bytes = 0
var total_blocks = 0
var gmax_bytes = 0

for pp in doc.pps {
  total_bytes += pp.tb
  total_blocks += pp.tbk
  gmax_bytes += pp.gb
}

let ranked = doc.pps
  |> sort-by --desc .tb
  |> take(top)

let sites = [{frames: site_frames(pp, doc.ftbl), total_bytes: pp.tb, max_bytes: pp.mb, total_blocks: pp.tbk} for pp in ranked]
print f"dhat allocation summary: ${scenario}"
print f"  total: ${total_bytes} bytes in ${total_blocks} blocks; peak (t-gmax): ${gmax_bytes} bytes"
print f"  top ${sites.len()} sites by total bytes:"
print ""
var rank = 1

for site in sites {
  print f"  #${rank}  ${site.total_bytes} bytes  ${site.total_blocks} blk  (max-live ${site.max_bytes})"
  print f"      ${first_meaningful(site.frames)}"
  rank += 1
}

if script_args.len() >= 3 {
  let out_path = fp"${script_args[2]}"

  let summary: DhatSummary = {
    version: 1,
    scenario,
    total_bytes,
    total_blocks,
    gmax_bytes,
    sites,
  }

  json.write(out_path, summary, pretty: true)?
  print ""
  print f"  json: ${out_path.display()}"
}
