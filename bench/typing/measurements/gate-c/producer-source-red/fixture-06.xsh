proc argument() [time] -> Int { let _ = time.now(); 1 }
stream inert(item: Int) [] -> Stream[Int] { yield item }
proc create() [] -> Unit { let _ = inert(argument()) }
