tempdir root {
  let src = fp"{root}/src"
  src.mkdir()
  let docs = fp"{root}/docs"
  docs.mkdir()

  fp"{src}/main.xsh".write(
    """print "hi"
""",
  )

  fp"{src}/lib.xsh".write(
    """pure id(value: Str) -> Str { value }
""",
  )

  fp"{docs}/README.md".write(
    """structured reports
""",
  )

  let reports = fs.files(root)
    |> where .kind == "file"
    |> map {
      {name: .name, size: .size, parent: .path.parent().name}
    }
    |> sort-by .name

  let labels = reports |> par-map f"{.parent}/{.name}:{.size}"

  let source_reports = reports |> where .parent == "src"

  print f"files {reports.len()} source {source_reports.len()}"
  print labels[0] labels[1] labels[2]
  print f"largest {reports[2].name} {reports[2].size}"
}
