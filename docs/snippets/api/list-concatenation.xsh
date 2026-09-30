var files: List[Str] = []
files += ["main.xsh"]
let earlier = files
files += ["helpers.xsh", "tests.xsh"]
let all_files = earlier + ["README.md"]
print files.join(",")
print all_files.join(",")
