let largest = 8589934591GiB
let too_large = 8589934592GiB # error: check.size-literal
print f"{largest} {too_large}"
