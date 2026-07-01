#!/usr/bin/env -S xsh --
# Flamegraph
# Render folded stack samples into an SVG flamegraph without external tooling.
# Usage: xsh showcase/flamegraph.xsh -- [FOLDED_STACKS] > flamegraph.svg
# Example: xsh showcase/flamegraph.xsh -- out.folded > flamegraph.svg
# Render a flamegraph SVG from folded-stack input.
#
# Input format (one line per sample):
#   frame1;frame2;leaf DURATION
#
# Usage:
#   xsht trace --trace-format flamegraph --trace-file out.txt SCRIPT
#   xsh showcase/flamegraph.xsh out.txt > flamegraph.svg
#
# With no argument, renders a built-in example.
type Frame = {key: Str, name: Str, depth: Int, count: Int}

pure frame_color(name: Str) -> Str {
  # Deterministic warm color: high red, variable green, low blue.
  let n = name.count_chars()
  let r = 205 + n * 37 % 50
  let g = n * 53 % 230
  let b = n * 71 % 55
  return f"rgb(${r},${g},${b})"
}

pure truncate_label(name: Str, max_chars: Int) -> Str {
  if max_chars < 2 {
    return ""
  }

  if name.count_chars() <= max_chars {
    return name
  }

  let chars = name.split("")
  let truncated = chars |> take(max_chars - 1)
  return f"${truncated.join("")}.."
}

pure parent_key(key: Str) -> Str {
  let parts = key.split(";")
  let depth = parts.len() - 1

  if depth == 0 {
    return ""
  }

  let prefix = parts |> take(depth)
  return prefix.join(";")
}

proc main(input: Str = "") [fs, error] {
  let sample = """script;proc:main;module.fs.walk 8500
script;proc:main;module.fs.stat 2000
script;proc:main;module.fs.stat 500
script;proc:main;run:git 3200
script;proc:main;|>where;method.Regex.matches 4100
script;proc:main;|>where;method.Regex.matches 2300
script;proc:main;|>map;module.hash.sha256 6800
script;proc:format 1200
"""

  let source = if input == "" { sample } else { fp"${input}".read_text()? }

  # Sum counts for identical stacks.
  var raw: Map[Int] = {}

  for line in source.lines()
    |> where .trim() != ""
    |> where ! .starts_with("#") {
    let parts = line.split(" ")
    let stack = parts[0]
    let count = json.decode(parts.get(1, "0"))?
    raw[stack] = raw.get(stack, 0) + count
  }

  # Expand each leaf stack into prefix cumulative counts.
  var cum: Map[Int] = {}

  for stack in raw.keys() {
    let n = raw.get(stack, 0)
    let frames = stack.split(";")

    for item in frames |> enumerate() {
      let prefix = frames |> take(item.index + 1)
      let key = prefix.join(";")
      cum[key] = cum.get(key, 0) + n
    }
  }

  if cum.keys().len() == 0 {
    print "no data"
    return
  }

  # Build frame list from cumulative counts.
  var all_frames: List[Frame] = []

  for key in cum.keys() {
    let parts = key.split(";")
    let frame: Frame = {key: key, name: parts[parts.len() - 1], depth: parts.len() - 1, count: cum.get(key, 0)}
    all_frames = all_frames.push(frame)
  }

  # Compute layout. Alphabetical key order is DFS preorder for semicolon-delimited
  # stacks because ';' (ASCII 59) sorts before any letter or digit.
  let sorted = all_frames |> sort-by .key

  # x_pos stores each frame's left edge (in count units).
  # x_cur tracks the current cursor within each parent for placing siblings.
  var x_cur: Map[Int] = {}
  var x_pos: Map[Int] = {}

  for f in sorted {
    let pk = parent_key(f.key)
    let x = x_cur.get(pk, 0)
    x_cur[pk] = x + f.count
    x_cur[f.key] = x
    x_pos[f.key] = x
  }

  let total = all_frames
    |> where .depth == 0
    |> map .count
    |> sum

  let max_depth = (all_frames
    |> map .depth
    |> max)?

  let svg_w = 1200
  let fh = 16
  let top_pad = 28
  let svg_h = top_pad + (max_depth + 1) * fh + 4
  print "<?xml version=\"1.0\" encoding=\"utf-8\"?>"
  print f"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"${svg_w}\" height=\"${svg_h}\" style=\"font-family:monospace\">"
  print f"  <text x=\"${svg_w / 2}\" y=\"18\" text-anchor=\"middle\" font-size=\"14\">Flamegraph</text>"

  for f in sorted {
    continue when total == 0
    let x_count = x_pos.get(f.key, 0)
    let x_px = x_count * (svg_w - 4) / total + 2
    let w_px = f.count * (svg_w - 4) / total
    continue when w_px < 1
    let y_px = top_pad + (max_depth - f.depth) * fh
    let fill = frame_color(f.name)
    print f"  <g>"
    print f"    <title>${f.name} (${f.count} µs)</title>"
    print f"    <rect x=\"${x_px}\" y=\"${y_px}\" width=\"${w_px}\" height=\"${fh - 1}\" fill=\"${fill}\" rx=\"2\"/>"

    if w_px >= 16 {
      let label = truncate_label(f.name, w_px / 7)

      if label != "" {
        print f"    <text x=\"${x_px + 3}\" y=\"${y_px + fh - 4}\" font-size=\"11\">${label}</text>"
      }
    }

    print "  </g>"
  }

  print "</svg>"
}
