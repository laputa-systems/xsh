#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

runs="${RUNS:-8}"
warmup="${WARMUP:-2}"
records="${RECORDS:-5000}"
out_dir="target/perf"
old_script="$out_dir/jq-old.xsh"
corpus="$out_dir/jq-${records}.json"
result="$out_dir/jq-type-patterns-hyperfine.json"

mkdir -p "$out_dir"

perl -0pe '
  s@# Decode a numeric token to a Float\. json\.decode collapses integral values to Int\n# \(regardless of literal syntax\) and keeps only genuinely-fractional values as Float\.\npure decode_num\(tok: Str\) -> Result\[Json\] \{\n  return match json\.decode\(tok\)\? \{\n    i is Int => Ok\(JNum\(i\.float\(\)\)\)\n    f is Float => Ok\(JNum\(f\)\)\n    _ => Err\(jq_err\("Invalid JSON number"\)\)\n  \}\n\}@# Decode a numeric token to a Float. json.decode collapses integral values to Int\n# (regardless of literal syntax) and keeps only genuinely-fractional values as Float;\n# discriminate by the canonical encoded form.\npure decode_num(tok: Str) -> Result[Json] {\n  let v = json.decode(tok)?\n  let enc = json.encode(v)?\n\n  if enc.contains(".") or enc.contains("e") or enc.contains("E") {\n    let f: Float = v\n    return Ok(JNum(f))\n  }\n\n  let i: Int = v\n  return Ok(JNum(i.float()))\n}@ or die "failed to build old jq decode_num baseline\n";
' showcase/jq.xsh > "$old_script"

perl -e '
  my $records = shift @ARGV;
  print "[";
  for my $i (0 .. $records - 1) {
    print "," if $i;
    my $active = ($i % 3 == 0) ? "true" : "false";
    my $price = ($i % 100) + 0.25;
    print qq({"id":$i,"active":$active,"price":$price,"name":"pkg$i"});
  }
  print "]\n";
' "$records" > "$corpus"

cargo build --release

hyperfine \
  --warmup "$warmup" \
  --runs "$runs" \
  --export-json "$result" \
  "target/release/xsh $old_script -- -c 'map(select(.active).price)|add' < $corpus >/dev/null" \
  "target/release/xsh showcase/jq.xsh -- -c 'map(select(.active).price)|add' < $corpus >/dev/null"

echo "wrote $result"
