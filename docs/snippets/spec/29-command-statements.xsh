const target = "release"
const metadata = {target: "release"}
# begin example
print "building" $target
json.write(p"out.json", metadata, pretty: true)?
# end example
