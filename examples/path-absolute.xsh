let cwd = fs.cwd()?
let dest = path.absolute(p"target/../target/package")?
let same = dest == fp"${cwd}/target/package"
print $same
