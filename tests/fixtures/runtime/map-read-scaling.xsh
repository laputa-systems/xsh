# Map reads must not copy the map.
#
# Construction and traversal are separated so the execution allocation count
# in `tests/runtime/collections.rs::map_reads_do_not_copy_the_map` can isolate
# the cost of each read at different map sizes.
#
# The harness supplies the number of fields and read passes.
proc main() [env, error, io] {
  let size = env.int("XSH_MAP_READ_FIELDS", 256)?
  let passes = env.int("XSH_MAP_READ_PASSES", 1)?
  var values: Map[Int] = map.empty()
  var index = 0
  while index < size {
    values = values.set(f"k${index}", index)
    index = index + 1
  }

  let keys = values.keys()
  var total = 0
  var pass = 0
  while pass < passes {
    for key in keys {
      total = total + values.get(key, 0)
    }

    pass = pass + 1
  }

  print f"${size} ${passes} ${total}"
}
