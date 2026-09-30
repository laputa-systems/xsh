pure check(value: Int) -> Result[Int] { value > 0; value }
print ${check(-1) is Err(_)} ${check(3)?}
