let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
let src = fp"${root}/src"
src.mkdir()
let docs = fp"${root}/docs"
docs.mkdir()

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

fs.write(
  fp"${docs}/README.md",
  """structured reports
""",
)?

let reports = fs.files(root)
  |> where .kind == "file"
  |> map { |entry|
    {name: entry.name, size: entry.size, parent: entry.path.parent().name}
  }
  |> sort-by .name

let labels = reports
  |> par-map { |value|
    f"${value.parent}/${value.name}:${value.size}"
  }

let source_reports = reports |> where .parent == "src"

print f"files ${reports.len()} source ${source_reports.len()}"
print labels[0] labels[1] labels[2]
print f"largest ${reports[2].name} ${reports[2].size}"
