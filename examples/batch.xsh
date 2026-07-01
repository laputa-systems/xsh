let objects = [p"build/main.o", p"build/lib.o", p"build/cli.o", p"build/test.o", p"build/doc.o"]
let pairs = objects |> batch --count=2
print pairs[0][0].name pairs[0][1].name
print pairs[2][0].name

let _linked = objects
  |> batch --max-argv
  |> each { |chunk|
    run true @chunk
  }

print "link argv ok"
