let sources = fs.files(p"src", exts: ["c"])?
  |> where .size > 0
  |> map .path
  |> sort

let total = fs.files(p"src")? |> map .size |> sum()
