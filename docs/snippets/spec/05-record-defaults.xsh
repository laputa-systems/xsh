const root = p"."

# begin example
type BuildOptions = {root: Path, jobs: UInt = 4, flags: List[Str] = []}

let opts = BuildOptions(p"src")
let wide = BuildOptions(root:, jobs: 16)
# end example
