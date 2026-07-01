let root = fs.tempdir()?

fs.root_write(
  root,
  p"main.c",
  """int main(void) { return 0; }
""",
)?

run printf "%s\n" "build ok"

fs.root_write(
  root,
  p"main.o",
  """object
""",
)?

let exists = fs.root_exists(root, p"main.o")?
print $exists
fs.close_root(root)?
