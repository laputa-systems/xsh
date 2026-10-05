let image = ["/tmp/image.tar"][0]

# begin example
atomically replace image as partial { # error: check.type-mismatch
  partial.write("image")
}
# end example
