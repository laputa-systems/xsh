stream delayed() [time, env] -> Stream[Int] {
  defer { let _ = env.get("UNREAD_SETTING") }
  let _ = time.now()
  yield 1
}
let rows = delayed()
proc consume() [time] -> Unit { for item in rows { let _ = item; break } }
