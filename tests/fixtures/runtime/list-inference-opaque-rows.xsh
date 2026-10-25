type Task = {name: Str, argv: List[Any]}
type Batch = {tasks: List[Task]}
pure produce() -> Batch { {tasks: [{name: "task", argv: [7]}]} }
pure one() -> Task { {name: "last", argv: [8]} }
pure count(tasks: List[Task]) -> Int { tasks.len() }
pure names(values: List[Str]) -> Int { values.len() }
proc build(splice: Bool) [] -> Int {
  var tasks = []
  let batch = produce()
  if splice {
    tasks = [@tasks, @batch.tasks, @batch.tasks]
  } else {
    tasks = tasks.extend(batch.tasks).extend(batch.tasks)
  }
  let named = names([task.name for task in tasks])
  count(tasks.push(one()))
}
print ${build(false)} ${build(true)}
