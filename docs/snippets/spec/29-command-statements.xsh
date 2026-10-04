const target = "release"
const metadata = {target: "release"}
# begin example
print "building" $target
fs.mkdir build
fs.remove dist --missing-ok
json.write out.json $metadata
# end example
