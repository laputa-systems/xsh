type Box = {values: List[Int]}
pure select(box) -> List[Int] {
  box.values
}
let values: List[Int] = select({values: [1, 2]})
