tempdir dir {
  let root = fp"{dir}/tree"
  fp"{root}/sub".mkdir()
  fp"{root}/.hidden".write("h")
  fp"{root}/.gitignore".write("*.log\n")
  fp"{root}/a.log".write("l")
  fp"{root}/sub/b.txt".write("b")
  fp"{root}/link".symlink(to: p"sub")
  let children = fs.children(root)? |> map { |e| f"{e.name}:{e.kind}" } |> sort()
  print (children.join(" "))
  let plain = fs.walk(root)? |> where .kind != "dir" |> map { |e| e.path.relative_to(root).display() } |> sort()
  print (plain.join(" "))
  let all = fs.walk(root, gitignore: false, hidden: true)? |> where .kind != "dir" |> map { |e| e.path.relative_to(root).display() } |> sort()
  print (all.join(" "))
  let first = fs.walk(root)? |> first()
  print f"{first?.path.starts_with(root)}"
}
