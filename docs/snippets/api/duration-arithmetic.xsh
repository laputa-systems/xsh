const budget = 1s + 500ms
const quantized = 5ms / 2

pure retry_delay(attempt: Int) -> Duration {
  250ms * attempt
}

let pause = retry_delay(3)
assert pause <= budget
let intervals = budget / 250ms
assert intervals == 6
assert quantized == 2ms
