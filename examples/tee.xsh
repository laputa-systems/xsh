let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
fs.write(fp"${root}/alpha.txt", "aaa")?
fs.write(fp"${root}/beta.txt", "bb")?
fs.write(fp"${root}/gamma.txt", "c")?

let sizes = fs.children(root)
  |> where .kind == "file"
  |> sort-by .path
  |> tee { |entry|
    print f"visit ${entry.path.name()}"
  }
  |> map .size

print sizes[0] sizes[1] sizes[2]
