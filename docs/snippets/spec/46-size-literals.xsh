const size: UInt = 3221225472
# begin example
const chunk = 64KiB
const reserve = 256MiB

pure fits(needed: UInt, free: UInt) -> Bool {
  needed + reserve <= free
}

print f"{size / 1MiB} MiB in {size / chunk} chunks; fits in 8GiB: {fits(size, 8GiB)}"
# end example
