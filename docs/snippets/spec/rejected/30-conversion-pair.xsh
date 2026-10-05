let ratio = 2.5
# begin example
let whole = ratio as Int  # error: check.conversion
let rounded = ratio.round()?
# end example
print $whole $rounded
