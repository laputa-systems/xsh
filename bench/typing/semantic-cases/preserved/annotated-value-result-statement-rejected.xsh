proc leaf() [] -> Result[Int] { Ok(1) }
proc bad() [error] -> Int { leaf(); 2 }
print ${bad()}
