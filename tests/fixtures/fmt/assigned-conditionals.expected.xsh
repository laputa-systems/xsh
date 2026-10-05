pure s16(offset: Int) -> Int { return offset }

pure s32(offset: Int) -> Int { return offset * 2 }

proc decode(width: Int, names: List[Str], count: Int, numbers_at: Int) -> Int {
  var numbers: Map[Str, Int] = {}
  var listed: List[Int] = []
  var total = 0
  if width > 0 {
    if count > 0 {
      # An assigned map comprehension whose value is a conditional.
      numbers = {
        [names[k]]: if width == 4 {
          s32(numbers_at + 4 * k + 1000000000000)
        } else {
          s16(numbers_at + 2 * k + 100000000)
        }
        for k in range(count)
      }
      numbers = {
        [names[k]]: if width == 4 {
          s32(numbers_at + 4 * k + 1000000000000)
        } else {
          s16(numbers_at + 2 * k + 100000000)
        }
        for k in range(count)
        if k > 2
      }
      # An assigned list comprehension with a filter, and a nested one.
      listed = [
        if width == 4 { s32(numbers_at + 4 * k + 10000000) } else { s16(numbers_at + 2 * k + 10000000) }
        for k in range(count)
        if k > 100
      ]
      listed = [
        [s32(numbers_at + 4 * k + 100000000 + j) for k in range(count) if k > 100000000].len()
        for j in range(count)
        if j != 1000000000
      ]
      # A conditional that is the assigned value, and one that is an operand.
      total = if width == 4 {
        s32(numbers_at + 4 * count + 100000000 + 100000000)
      } else {
        s16(numbers_at + 2 * count + 100000000)
      }
      total += 1 + (if width == 4 {
        s32(numbers_at + 4 * count + 100000000 + 100000000)
      } else {
        s16(numbers_at + 2 * count + 1000)
      })
    }
  }

  return total + numbers.len() + listed.len()
}

let decoded = decode(4, ["a", "b", "c", "d"], 4, 0)
print f"assigned conditionals preserved: {decoded}"
