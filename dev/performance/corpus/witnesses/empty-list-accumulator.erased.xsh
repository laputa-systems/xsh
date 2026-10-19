pure collect_numbers(numbers: List[Int]) -> List[Int] {
  var values = []
  for number in numbers {
    values = values.push(number)
  }
  values
}

let collected = collect_numbers([3, 5, 8])
print ${collected.len()} ${collected[0]} ${collected[1]} ${collected[2]}
