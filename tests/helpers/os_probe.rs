#![allow(clippy::single_call_fn)]

use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static TRAPPED: AtomicBool = AtomicBool::new(false);

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(mode) = args.next().and_then(|arg| arg.into_string().ok()) else {
        fatal("mode is required");
    };

    let result = match mode.as_str() {
        "delayed-marker" => delayed_marker(args),
        "fork-new-session-leak" => fork_new_session_leak(args),
        "group-leak" => group_leak(args),
        "ready-sleep" => ready_sleep(args),
        "self-signal" => self_signal(args),
        "signal-parent-after" => signal_parent_after(args, false),
        "signal-parent-sequence" => signal_parent_sequence(args),
        "signal-parent-then-sleep" => signal_parent_after(args, true),
        "trap-and-wait" => trap_and_wait(args),
        _ => Err(format!("unknown mode `{mode}`")),
    };

    if let Err(message) = result {
        fatal(&message);
    }
}

fn delayed_marker(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let marker = required_path(&mut args, "marker")?;
    let delay = required_duration_ms(&mut args, "delay_ms")?;
    std::thread::sleep(delay);
    write_file(&marker, b"ready")
}

fn fork_new_session_leak(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let ready = required_path(&mut args, "ready")?;
    let leak = required_path(&mut args, "leak")?;
    let child = unsafe { libc::fork() };
    if child < 0 {
        return Err(format!("fork failed: {}", io::Error::last_os_error()));
    }
    if child == 0 {
        if unsafe { libc::setsid() } < 0 {
            fatal(&format!("setsid failed: {}", io::Error::last_os_error()));
        }
        let pid = unsafe { libc::getpid() };
        write_file(&ready, format!("{pid}\n").as_bytes()).unwrap_or_else(|error| fatal(&error));
        std::thread::sleep(Duration::from_millis(700));
        write_file(&leak, b"leaked").unwrap_or_else(|error| fatal(&error));
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    ignore_signal(libc::SIGTERM)?;
    ignore_signal(libc::SIGINT)?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn group_leak(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let ready = required_path(&mut args, "ready")?;
    let leak = required_path(&mut args, "leak")?;
    let child = unsafe { libc::fork() };
    if child < 0 {
        return Err(format!("fork failed: {}", io::Error::last_os_error()));
    }
    if child == 0 {
        ignore_signal(libc::SIGTERM).unwrap_or_else(|error| fatal(&error));
        ignore_signal(libc::SIGINT).unwrap_or_else(|error| fatal(&error));
        std::thread::sleep(Duration::from_secs(2));
        write_file(&leak, b"leaked").unwrap_or_else(|error| fatal(&error));
        std::process::exit(0);
    }

    write_file(&ready, b"ready")?;
    ignore_signal(libc::SIGTERM)?;
    ignore_signal(libc::SIGINT)?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn ready_sleep(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let marker = required_path(&mut args, "marker")?;
    write_file(&marker, b"ready")?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn self_signal(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let signal = required_signal(&mut args)?;
    let marker = required_path(&mut args, "marker")?;
    write_file(&marker, b"ready")?;
    let rc = unsafe { libc::kill(libc::getpid(), signal) };
    if rc != 0 {
        return Err(format!("kill self failed: {}", io::Error::last_os_error()));
    }
    std::thread::sleep(Duration::from_millis(100));
    Ok(())
}

fn signal_parent_sequence(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let first_signal = required_signal(&mut args)?;
    let first_delay = required_duration_ms(&mut args, "first_delay_ms")?;
    let second_signal = required_signal(&mut args)?;
    let second_delay = required_duration_ms(&mut args, "second_delay_ms")?;
    std::thread::sleep(first_delay);
    kill_parent(first_signal)?;
    std::thread::sleep(second_delay);
    kill_parent(second_signal)?;
    Ok(())
}

fn signal_parent_after(
    mut args: impl Iterator<Item = OsString>,
    sleep_after: bool,
) -> Result<(), String> {
    let signal = required_signal(&mut args)?;
    let delay = required_duration_ms(&mut args, "delay_ms")?;
    std::thread::sleep(delay);
    kill_parent(signal)?;
    if sleep_after {
        std::thread::sleep(Duration::from_secs(2));
    }
    Ok(())
}

fn kill_parent(signal: i32) -> Result<(), String> {
    let rc = unsafe { libc::kill(libc::getppid(), signal) };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "kill parent failed: {}",
            io::Error::last_os_error()
        ))
    }
}

fn trap_and_wait(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let ready = required_path(&mut args, "ready")?;
    let caught = required_path(&mut args, "caught")?;
    let signal = required_signal(&mut args)?;
    install_trap(signal)?;
    write_file(&ready, b"ready")?;
    loop {
        if TRAPPED.load(Ordering::SeqCst) {
            write_file(&caught, b"caught")?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn required_path(
    args: &mut impl Iterator<Item = OsString>,
    name: &str,
) -> Result<OsString, String> {
    args.next().ok_or_else(|| format!("{name} is required"))
}

fn required_signal(args: &mut impl Iterator<Item = OsString>) -> Result<i32, String> {
    let signal = args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .ok_or_else(|| "signal is required".to_string())?;
    signal_number(&signal).ok_or_else(|| format!("unsupported signal `{signal}`"))
}

fn required_duration_ms(
    args: &mut impl Iterator<Item = OsString>,
    name: &str,
) -> Result<Duration, String> {
    let value = args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .ok_or_else(|| format!("{name} is required"))?;
    let millis = value
        .parse::<u64>()
        .map_err(|error| format!("invalid {name}: {error}"))?;
    Ok(Duration::from_millis(millis))
}

fn signal_number(signal: &str) -> Option<i32> {
    match signal.strip_prefix("SIG").unwrap_or(signal) {
        "HUP" => Some(libc::SIGHUP),
        "INT" => Some(libc::SIGINT),
        "QUIT" => Some(libc::SIGQUIT),
        "TERM" => Some(libc::SIGTERM),
        "USR1" => Some(libc::SIGUSR1),
        "USR2" => Some(libc::SIGUSR2),
        "ALRM" => Some(libc::SIGALRM),
        _ => None,
    }
}

fn write_file(path: impl AsRef<Path>, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

fn install_trap(signal: i32) -> Result<(), String> {
    unsafe extern "C" fn trap(_: i32) {
        TRAPPED.store(true, Ordering::SeqCst);
    }

    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = trap as *const () as usize;
    unsafe {
        libc::sigemptyset(&mut action.sa_mask);
    }
    let rc = unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!("sigaction failed: {}", io::Error::last_os_error()))
    }
}

fn ignore_signal(signal: i32) -> Result<(), String> {
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = libc::SIG_IGN;
    unsafe {
        libc::sigemptyset(&mut action.sa_mask);
    }
    let rc = unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "sigaction ignore failed: {}",
            io::Error::last_os_error()
        ))
    }
}

fn fatal(message: &str) -> ! {
    eprintln!("xsh-test-os-probe: {message}");
    std::process::exit(2);
}
