type Box = {values: List[Int]}
pure select(box: Box) -> List[Int] {
  box.values
}
let values: List[Int] = select({values: [1, 2]})
