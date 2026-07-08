#!/usr/bin/env -S xsh --
# JSON Diff
# Compare two JSON documents and report added, removed, changed, and unchanged keys.
# Usage: xsh showcase/json-diff.xsh -- OLD.json NEW.json
# Example: xsh showcase/json-diff.xsh -- before.json after.json
type Opts = {a: Path, b: Path}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(argv, {a: {form: "A", kind: "Path", file: true}, b: {form: "B", kind: "Path", file: true}})?
  let json_a = json.read(opts.a.resolve()?)?.require(Map[Any])?
  let json_b = json.read(opts.b.resolve()?)?.require(Map[Any])?
  let keys_a = json_a.keys()
  let keys_b = json_b.keys()
  let removed: List[Str] = keys_a |> where ! json_b.has(.)
  let added: List[Str] = keys_b |> where ! json_a.has(.)
  let common: List[Str] = keys_a |> where json_b.has(.)
  var changed: List[Str] = []
  var same = 0

  for key in common {
    let va = json.encode(json_a.get(key)?)?
    let vb = json.encode(json_b.get(key)?)?

    if va != vb {
      changed = changed.push(key)
    } else {
      same += 1
    }
  }

  print f"a: ${opts.a.display()}  (${keys_a.len()} keys)"
  print f"b: ${opts.b.display()}  (${keys_b.len()} keys)"
  print ""

  if removed.len() == 0 and added.len() == 0 and changed.len() == 0 {
    print "identical top-level structure"
    return
  }

  if removed.len() > 0 {
    print f"removed (${removed.len()}):"

    for key in removed {
      let v = json.encode(json_a.get(key)?)?
      print f"  - ${key}: ${v}"
    }

    print ""
  }

  if added.len() > 0 {
    print f"added (${added.len()}):"

    for key in added {
      let v = json.encode(json_b.get(key)?)?
      print f"  + ${key}: ${v}"
    }

    print ""
  }

  if changed.len() > 0 {
    print f"changed (${changed.len()}):"

    for key in changed {
      let va = json.encode(json_a.get(key)?)?
      let vb = json.encode(json_b.get(key)?)?
      print f"  ~ ${key}"
      print f"    < ${va}"
      print f"    > ${vb}"
    }

    print ""
  }

  print f"same ${same}  removed ${removed.len()}  added ${added.len()}  changed ${changed.len()}"
}
