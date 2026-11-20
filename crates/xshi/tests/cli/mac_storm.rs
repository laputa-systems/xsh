use std::ffi::CStr;
use std::path::Path;

pub fn live_processes(dir: &Path) -> Vec<(i32, String)> {
    let reported = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    assert!(reported >= 0, "proc_listallpids: {}", std::io::Error::last_os_error());
    let mut capacity = (reported as usize).max(1);
    let pids = loop {
        let mut pids = vec![0i32; capacity];
        let bytes = capacity.checked_mul(std::mem::size_of::<i32>()).unwrap();
        let count = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes.try_into().unwrap()) };
        assert!(count >= 0, "proc_listallpids: {}", std::io::Error::last_os_error());
        if (count as usize) < capacity {
            pids.truncate(count as usize);
            break pids;
        }
        capacity = capacity.checked_mul(2).unwrap();
    };
    let mut found = Vec::new();
    for pid in pids {
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&info);
        if unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&mut info as *mut libc::proc_bsdinfo).cast(), size as i32) } != size as i32 || info.pbi_status == libc::SZOMB {
            continue;
        }
        let mut paths: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&paths);
        if unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDVNODEPATHINFO, 0, (&mut paths as *mut libc::proc_vnodepathinfo).cast(), size as i32) } != size as i32 {
            continue;
        }
        let cwd = unsafe { CStr::from_ptr(paths.pvi_cdir.vip_path.as_ptr().cast()) };
        use std::os::unix::ffi::OsStrExt;
        if Path::new(std::ffi::OsStr::from_bytes(cwd.to_bytes())) == dir {
            let name = unsafe { CStr::from_ptr(info.pbi_comm.as_ptr()) }.to_string_lossy().into_owned();
            found.push((pid, name));
        }
    }
    found
}
