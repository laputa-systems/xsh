#!/usr/bin/env -S xsh --
# JSON Diff
# Compare two JSON objects and report added, removed, changed, and unchanged keys.
# Usage: xsh showcase/json-diff.xsh -- OLD.json NEW.json
# Example: xsh showcase/json-diff.xsh -- before.json after.json
type Opts = {a: Path, b: Path}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(argv, {a: {form: "A", kind: "Path", file: true}, b: {form: "B", kind: "Path", file: true}})?
  let json_a = json.read(opts.a.resolve()?)?.require(Record)?
  let json_b = json.read(opts.b.resolve()?)?.require(Record)?
  let keys_a = json_a.keys()
  let keys_b = json_b.keys()
  let removed = keys_a
    |> where { |key|
      key not in json_b
    }
  let added = keys_b
    |> where { |key|
      key not in json_a
    }
  let common = keys_a
    |> where { |key|
      key in json_b
    }
  var changed = []
  var same = 0

  for key in common {
    let va = json.encode(json_a.get(key)?)?
    let vb = json.encode(json_b.get(key)?)?

    if va != vb {
      changed += [key]
    } else {
      same += 1
    }
  }

  print f"a: {opts.a}  ({keys_a.len()} keys)"
  print f"b: {opts.b}  ({keys_b.len()} keys)"
  print ""

  if removed.is_empty() and added.is_empty() and changed.is_empty() {
    print "identical top-level structure"
    return
  }

  if ! removed.is_empty() {
    print f"removed ({removed.len()}):"

    for key in removed {
      let v = json.encode(json_a.get(key)?)?
      print f"  - {key}: {v}"
    }

    print ""
  }

  if ! added.is_empty() {
    print f"added ({added.len()}):"

    for key in added {
      let v = json.encode(json_b.get(key)?)?
      print f"  + {key}: {v}"
    }

    print ""
  }

  if ! changed.is_empty() {
    print f"changed ({changed.len()}):"

    for key in changed {
      let va = json.encode(json_a.get(key)?)?
      let vb = json.encode(json_b.get(key)?)?
      print f"  ~ {key}"
      print f"    < {va}"
      print f"    > {vb}"
    }

    print ""
  }

  print f"same {same}  removed {removed.len()}  added {added.len()}  changed {changed.len()}"
}
