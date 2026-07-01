# Demonstrates fs.gitroot() — walks up from cwd to find the nearest .git directory.
let root = fs.gitroot()?
print f"repo root: ${root.display()}"

# .git exists at the root:
let has_git = fs.exists(fp"${root}/.git")?
print $has_git
