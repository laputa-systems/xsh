type OpaqueTailRow = {name: Str, opaque: Any}

pure opaque_tail_name(row) -> Str { row.name }

proc verify_opaque_record_rows() -> Result[Unit] {
  let opaque: Any = {hidden: true}
  let row: OpaqueTailRow = {name: "kept", opaque: opaque}
  assert opaque_tail_name(row) == "kept"
  Ok()
}
