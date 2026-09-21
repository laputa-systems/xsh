# Record and map reads must not copy the container they read.
#
# The run separates construction from traversal: the map is built once, its
# keys are taken once, and then every key is read `XSH_RECORD_READ_PASSES`
# times. Reading cannot change the container, so a read that copied it would
# charge the traversal for the copy, and the allocation traffic the execution
# phase reports would grow with the number of passes and super-linearly with the
# size of the container. Both are what
# `tests/runtime/collections.rs::record_reads_do_not_copy_the_container`
# measures with `xsh-runtime-stats`, which owns the counting allocator a native
# XSH test cannot install.
#
# The two parameters are the fixture's inputs, supplied by that harness: the
# number of fields to build, and how many times every field is read.
proc main() [env, error, io] {
  let size = env.int("XSH_RECORD_READ_FIELDS", 256)?
  let passes = env.int("XSH_RECORD_READ_PASSES", 1)?
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
