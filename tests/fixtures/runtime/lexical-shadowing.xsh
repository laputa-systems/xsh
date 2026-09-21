# A declaration in a nested scope that shares a spelling with a binding an
# enclosing scope made. The checker accepts it (and `xsht lint` reports it as
# shadowing), so the lowerer must resolve the inner name to the inner binding
# and restore the outer binding when the inner scope ends.
#
# The file lives under `tests/fixtures/` because the runnable corpus excludes
# fixtures, and the lint policy forbids shadowing in corpus source; the program
# itself is an ordinary script executed by the ordinary runtime.

# A loop body that declares the same name the enclosing body declared, with the
# outer binding read again after the loop. Without correct slot identity the
# inner declaration is dropped and every read keeps the outer value.
pure shadowed_words(text: Str) -> Str {
  var out = ""
  var index = 0
  while index < text.byte_len() {
    let byte = text.byte_at(index, -1)
    if byte == 32 {
      index = index + 1
      continue
    }
    var word = ""
    while index < text.byte_len() {
      let byte = text.byte_at(index, -1)
      if byte == 32 {
        break
      }
      word = word + text.byte_slice(index, 1)
      index = index + 1
    }
    out = out + word + ";"
  }
  return out
}

# A block that is not a loop: the inner binding reads its own initializer, and
# the outer name is unchanged after the block.
pure shadowed_in_block(value: Str, inner: Str) -> Str {
  var name = value
  if name.byte_len() > 0 {
    let name = inner
    if name.byte_len() > 0 {
      return name
    }
  }
  return name
}

# The inner binding is mutable on its own: assigning to it changes only the
# inner binding, and the outer name keeps the value it had.
pure shadowed_assignment(value: Int, inner: Int) -> Int {
  var total = value
  if total > 0 {
    var total = inner
    total = total + 1
    if total > 0 {
      return total * 100
    }
  }
  return total
}

pure shadowed_loop_variable(items: List[Str]) -> Str {
  var item = "outer"
  var out = ""
  for item in items {
    out = out + item
  }
  return f"${out}|${item}"
}

proc main() [io, error] {
  print shadowed_words("ab cd")
  print shadowed_words("one")
  print shadowed_words(" a ")
  print shadowed_in_block("outer", "inner")
  print shadowed_in_block("outer", "")
  print f"${shadowed_assignment(7, 1)}"
  print shadowed_loop_variable(["a", "b"])
  print shadowed_loop_variable([])
}
