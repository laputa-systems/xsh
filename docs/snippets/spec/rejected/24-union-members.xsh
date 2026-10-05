type Word = Union[Str, Path]

type Count = Union[Int, UInt] # error: check.union-type
type Loose = Union[Str, Any] # error: check.union-type
type Maybe = Union[Str, Int?] # error: check.union-type
type Nested = Union[Word, Int] # error: check.union-type

let count: Union[Int, Float] = 1
print ${count == 1}
