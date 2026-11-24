type Input = {value: Int, offset: Int}
pure step(input: Input) -> Int { input.value * 2 + input.offset }
var total = 0
for i in range(200000) {
  total += step(Input(value: i, offset: 1))
}
print $total
