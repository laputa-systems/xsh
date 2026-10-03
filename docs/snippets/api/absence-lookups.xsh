let offset = "\u{e9}: data".find(":")

if offset != null {
  print ${offset}
}

let first_byte = "\u{e9}".byte_at(0) ?? 0
const entries: Map[Int?] = {present: null}
let present = entries.get("present") ?? 7
let missing = entries.get("missing") ?? 7

if present == null {
  print "present null"
}

if missing != null {
  print ${first_byte} ${missing}
}
