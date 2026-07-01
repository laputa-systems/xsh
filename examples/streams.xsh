let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
let src = fp"${root}/src"
src.mkdir()

fs.write(
  fp"${src}/main.xsh",
  """print "hi"
""",
)?

fs.write(
  fp"${src}/lib.xsh",
  """pure id(value: Str) -> Str { value }
""",
)?

let files = fs.files(root) |> sort-by .name
let names = files |> map .name

let reports = files
  |> par-map { |value|
    f"${value.name}:${value.size}"
  }

print names[0] names[1]
print reports[0] reports[1]
