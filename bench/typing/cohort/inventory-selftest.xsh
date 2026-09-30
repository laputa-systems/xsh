# π keeps byte spans distinct from character offsets.
type Schema = {field: Str}
proc annotated(value: Str) [] -> Result[Str] { value }
stream values() [error] -> Stream[Int] { yield 1 }
let local: Int = 1
let program: Str = "proc inside(value: Bool) [io] -> Unit { true }"
test checked [error] { |ctx|
  true
}
