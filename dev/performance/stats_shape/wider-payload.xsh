type Stats = {blanks: Int, code: List[Int], comments: Int, blobs: Map[Int]}
var total = 0
var kept: List[Stats] = []
for i in range(200000) {
  let record = Stats(blanks: i, code: [i], comments: i, blobs: {})
  total += record.blanks + record.code[0] + record.comments
  if i % 1000 == 0 { kept += [record] }
}
print $total kept.len()
