type Task = {argv: List[Any]}
type Target = {tasks: List[Task]}
pure target() -> Target { {tasks: [{argv: [1]}]} }
pure count(tasks: List[Task]) -> Int { tasks.len() }
proc build() [] -> Int {
  var tasks = []
  for name in ["first", "second"] {
    let result = target()
    tasks = [@tasks, @result.tasks]
  }
  count(tasks)
}
print ${build()}
