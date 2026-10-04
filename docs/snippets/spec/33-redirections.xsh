const tarball = p"src.tar.gz"
const log = p"build.log"
const errlog = p"build.err"
const input = p"words"
const output = p"words.sorted"
const patch_bytes = b"--- a\n+++ b\n"
# begin example
run tar cf - src | run gzip -9 > $tarball
run make > $log 2> $errlog
run sort < $input > $output
run tool >& 2
run patch -p1 < $patch_bytes
run CC=cc CFLAGS="-O2 -pipe" ./configure --prefix=/usr
# end example
