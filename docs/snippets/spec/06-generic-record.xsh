# begin example
type Observation[T] = {value: T?, samples: List[T]}
type Count = Observation[Int]
let obs = Observation(value: 12, samples: [])
# end example
let counted: Count = obs
