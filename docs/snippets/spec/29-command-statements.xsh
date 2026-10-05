const target = "release"
const metadata = {target: "release"}
# begin example
print "building" $target
json.write out.json $metadata --pretty
# end example
