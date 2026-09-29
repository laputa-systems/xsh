pure backoff(attempt: Int) -> Duration { 250ms * attempt }

let budget = 1s + 500ms
let pause = backoff(3)
pause <= budget
let intervals: Int = budget / 250ms
intervals == 6
let quantized: Duration = 5ms / 2
quantized == 2ms
