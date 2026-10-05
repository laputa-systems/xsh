let stage = ["/tmp/stage"][0]

# begin example
tempdir scratch at stage { # error: check.type-mismatch
  print $scratch
}
# end example
