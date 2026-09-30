const budget = 1s + 500ms
const quantized = 5ms / 2

pure backoff(attempt: Int) -> Duration { 250ms * attempt }

let pause = backoff(3)
pause <= budget
let intervals = budget / 250ms
intervals == 6
quantized == 2ms
