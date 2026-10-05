const jobs = 8
# begin example
run (
  make
  "ARCH=arm64"
  "-j${jobs}"
  Image
)
# end example
