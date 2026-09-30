let offset = "é: data".find(":")
if offset != null {
  print ${offset}
}
let first_byte = "é".byte_at(0) ?? 0
const entries: Map[Int?] = {present: null}
let present = entries.get("present") ?? 7
let missing = entries.get("missing") ?? 7
if present == null { print "present null" }
if missing != null { print ${first_byte} ${missing} }
