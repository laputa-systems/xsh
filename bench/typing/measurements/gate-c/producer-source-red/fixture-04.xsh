proc argument() [time] -> Int { let _ = time.now(); 1 }
stream delayed(item: Int = argument()) [time] -> Stream[Int] { yield item }
proc create() [] -> Unit { let _ = delayed() }
