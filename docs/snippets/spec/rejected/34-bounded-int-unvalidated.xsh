type Port = Int range 1..=65535

pure listen(port: Port) -> Port {
  port
}

let number = 8080
let zero: Port = 0 # error: check.validated-literal
let high: Port = 65536 # error: check.validated-literal
let plain: Port = number # error: check.type-mismatch
let sum: Port = listen(80) + 1 # error: check.type-mismatch
var current: Port = 80
current += 1 # error: check.operator-type
print ${listen(number)} # error: check.type-mismatch
print $zero $high $plain $sum $current
