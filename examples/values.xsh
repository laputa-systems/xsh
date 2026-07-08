let name = "world"
let greeting = f"hello ${name}"

let banner = f"""${greeting}
from xsh"""

let roots = [/usr, /usr/local]
let scores = [2, 3, 5]
let ratio = scores[2].float() / 2.0
let release = {name: "xsh", root: roots[1], enabled: true}
print banner.lines().collect()[0]
print scores.len() ${scores[0] + scores[1] + scores[2]}
print ratio.format(precision: 2) (ratio.floor()?)
print roots[0].name $release.root.name $release.enabled
