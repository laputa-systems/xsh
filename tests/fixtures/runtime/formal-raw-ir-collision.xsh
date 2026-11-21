type FormalRawIrCollisionPoint = {formal_raw_ir_collision_x: Int, formal_raw_ir_collision_y: Int}

pure formal_raw_ir_collision_sum(formal_raw_ir_collision_point: FormalRawIrCollisionPoint) -> Int {
  formal_raw_ir_collision_point.formal_raw_ir_collision_x + formal_raw_ir_collision_point.formal_raw_ir_collision_y
}

let formal_raw_ir_collision_total = formal_raw_ir_collision_sum({formal_raw_ir_collision_x: 2, formal_raw_ir_collision_y: 40})
print f"{formal_raw_ir_collision_total}"
