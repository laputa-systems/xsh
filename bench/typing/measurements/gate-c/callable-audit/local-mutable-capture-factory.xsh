proc factory(initial) [] { var stored = initial; proc access() [] { stored }; (access) }
let first = factory(7)
let second = factory("word")
let integer: Int = first()
let text: Str = second()
