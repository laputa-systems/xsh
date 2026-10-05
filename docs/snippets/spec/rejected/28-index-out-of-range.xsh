# begin example
let names = ["build", "test"]
print names[-1]
let missing = ["build", "test"][-3]  # error: check.index-out-of-range
# end example
print $missing
