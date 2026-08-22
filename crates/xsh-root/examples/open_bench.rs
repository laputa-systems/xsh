use std::fs;
use std::hint::black_box;
use std::io;
use std::time::Instant;
use xsh_root::Root;

fn main() -> io::Result<()> {
    let root_path = std::env::temp_dir().join(format!(
        "xsh-root-open-bench-{}",
        std::process::id()
    ));
    let shallow = root_path.join("shallow");
    let deep = root_path.join("a/b/c/d/e/file");
    fs::create_dir_all(deep.parent().expect("deep path has a parent"))?;
    fs::write(&shallow, "benchmark")?;
    fs::write(&deep, "benchmark")?;
    let root = Root::open(&root_path)?;

    benchmark("std shallow", || fs::File::open(&shallow));
    benchmark("root shallow", || root.open_file("shallow"));
    benchmark("root deep", || root.open_file("a/b/c/d/e/file"));
    benchmark("std deep", || fs::File::open(&deep));

    fs::remove_dir_all(root_path)?;
    Ok(())
}

fn benchmark(name: &str, open: impl Fn() -> io::Result<fs::File>) {
    const ITERS: usize = 100_000;
    let started = Instant::now();
    for _ in 0..ITERS {
        black_box(open().expect("benchmark open"));
    }
    println!("{name}: {:?} for {ITERS} opens", started.elapsed());
}
