let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
fs.write(fp"${root}/small.txt", "a")?
fs.write(fp"${root}/large.txt", "abcd")?

fs.ls(root)
  |> sort-by .size
  |> table.print(columns: ["name", "size", "kind"])

let process_count = process.list()
  |> where .pid > 0
  |> count()

let has_processes = process_count > 0
print $has_processes
