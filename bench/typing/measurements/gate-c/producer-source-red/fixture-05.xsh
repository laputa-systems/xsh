proc argument() [time] -> Int { let _ = time.now(); 1 }
stream inert(item: Int) [] -> Stream[Int] { yield item }
proc create() [time] -> Unit { let _ = inert(argument()) }
