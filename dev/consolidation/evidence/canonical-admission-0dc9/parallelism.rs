use std::fs;
use std::thread;

fn main() {
    let parallelism = thread::available_parallelism().map(usize::from);
    println!("available_parallelism={parallelism:?}");
    for (label, path) in [
        ("cgroup", "/proc/self/cgroup"),
        ("cpu.max", "/sys/fs/cgroup/cpu.max"),
        ("cpuset.cpus.effective", "/sys/fs/cgroup/cpuset.cpus.effective"),
        ("status.cpus_allowed_list", "/proc/self/status"),
    ] {
        match fs::read_to_string(path) {
            Ok(contents) if label == "status.cpus_allowed_list" => {
                let value = contents.lines().find(|line| line.starts_with("Cpus_allowed_list:"));
                println!("{label}={value:?}");
            }
            Ok(contents) => println!("{label}={:?}", contents.trim()),
            Err(error) => println!("{label}=ERROR:{error}"),
        }
    }
}
