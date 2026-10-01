stream delayed()  -> Stream[Int] {
  defer { let _ = env.get("UNREAD_SETTING") }
  let _ = time.now()
  yield 1
}
proc create() [] -> Unit { let _ = delayed() }
