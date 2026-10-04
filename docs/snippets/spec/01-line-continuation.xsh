const config = {enabled: true}
const target = p"build"
# begin example
let files = fs.files(p"src")?
  |> where .ext == "rs"
  |> sort-by .path

let ready = config.enabled
  and target.exists()?
# end example
print f"{files.len()} sources, ready: {ready}"
