# Demonstrates utils.cache - memoizes a proc or pure call for the process lifetime.
# The key is derived automatically from the function name and argument values.
proc repo_root() [fs, error] -> Result[Str, Error] {
  let r = fs.gitroot()?
  r.display()
}

# First call runs the proc; subsequent calls return the cached value.
let first = utils.cache(repo_root)?
let second = utils.cache(repo_root)?
print f"root: ${first}"
print (first == second)

# With arguments: each distinct set of args is a separate cache entry.
pure greet(name: Str) -> Str {
  f"hello, ${name}"
}

let a = utils.cache(greet, ["world"])

# cache hit - greet not called again
let b = utils.cache(greet, ["world"])

# cache miss - different key
let c = utils.cache(greet, ["xsh"])
print $a
print (b == a)
print $c
