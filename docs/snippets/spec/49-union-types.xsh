# begin example
type Word = Union[Str, Path]

type Task = {tool: Str, args: List[Word]}

pure describe(word: Word) -> Str {
  match word {
    text is Str => f"text {text}"
    file is Path => f"file {file.name()}"
  }
}

proc build(root: Path) [process, error] {
  let task = Task("make", ["-C", root, "all"])
  run $task.tool @(task.args) ?

  for word in task.args {
    if word is Path {
      print f"in {word}"
      continue
    }

    print f"word {word}"
  }
}

# end example

print describe("all")
build(fs.cwd()?)
