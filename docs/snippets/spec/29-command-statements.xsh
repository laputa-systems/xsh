const target = "release"
const metadata = {target: "release"}
# begin example
print "building" $target
fs.mkdir build
fs.remove dist
json.write out.json $metadata --pretty
# end example
