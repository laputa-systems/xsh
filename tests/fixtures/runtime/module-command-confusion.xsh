use fs
let data = p"Cargo.toml".read_bytes()?
print data.len()
