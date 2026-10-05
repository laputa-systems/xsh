const jobs = 8
const log = p"build.log"
# begin example
run muon setup \
  -Ddefault_library=shared \
  -Dtests=false \
  build

run make "ARCH=arm64" -j${jobs} Image \
  > $log
# end example
