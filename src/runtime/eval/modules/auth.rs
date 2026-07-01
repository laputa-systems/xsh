#![allow(clippy::single_call_fn)]

use std::env;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_PATH: &str = "/usr/bin:/sbin:/bin";
const DEFAULT_SHELL: &str = "/bin/sh";

unsafe extern "C" {
    fn crypt(key: *const libc::c_char, salt: *const libc::c_char) -> *mut libc::c_char;
}

#[derive(Clone, Debug)]
pub(crate) struct SessionUser {
    pub(crate) name: String,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) home: PathBuf,
    pub(crate) shell: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Algorithm {
    Des,
    Md5,
    Sha256,
    Sha512,
}

pub(crate) fn verify_password(password: &str, hash: &str) -> bool {
    if hash.is_empty() || hash.starts_with('!') || hash.starts_with('*') {
        return false;
    }
    let Ok(password) = CString::new(password) else {
        return false;
    };
    let Ok(hash_c) = CString::new(hash) else {
        return false;
    };
    let computed = unsafe { crypt(password.as_ptr(), hash_c.as_ptr()) };
    if computed.is_null() {
        return false;
    }
    unsafe { CStr::from_ptr(computed) }.to_bytes() == hash_c.as_bytes()
}

pub(crate) fn hash_password(password: &str, algorithm: &str) -> Result<String, String> {
    let algorithm = parse_algorithm(algorithm)?;
    let salt = build_salt(algorithm)?;
    let password = CString::new(password).map_err(|_| "password contains NUL byte".to_string())?;
    let salt_c = CString::new(salt).map_err(|_| "salt contains NUL byte".to_string())?;
    let hashed = unsafe { crypt(password.as_ptr(), salt_c.as_ptr()) };
    if hashed.is_null() {
        return Err("failed to hash password".to_string());
    }
    Ok(unsafe { CStr::from_ptr(hashed) }
        .to_string_lossy()
        .into_owned())
}

pub(crate) fn login_session(user: &SessionUser, preserve_env: bool, host: &str) -> io::Result<i32> {
    let shell = default_shell(&user.shell);
    let mut command = Command::new(&shell);
    command.arg0(format!("-{}", login_name(&shell)));
    command.env_clear();
    if preserve_env {
        for (key, value) in env::vars_os() {
            command.env(key, value);
        }
    }
    if let Ok(term) = env::var("TERM") {
        command.env("TERM", term);
    }
    if preserve_env {
        if env::var_os("PATH").is_none() {
            command.env("PATH", DEFAULT_PATH);
        }
    } else {
        command.env("HOME", &user.home);
        command.env("SHELL", &shell);
        command.env("USER", &user.name);
        command.env("LOGNAME", &user.name);
        command.env(
            "PATH",
            env::var("PATH").unwrap_or_else(|_| String::from(DEFAULT_PATH)),
        );
    }
    if !host.is_empty() {
        command.env("REMOTEHOST", host);
    }

    let child_user = user.clone();
    let home = user.home.clone();
    unsafe {
        command.pre_exec(move || {
            drop_privileges(&child_user)?;
            env::set_current_dir(&home)?;
            Ok(())
        });
    }

    command.status().map(exit_code)
}

pub(crate) fn su_session(
    user: &SessionUser,
    login: bool,
    preserve_env: bool,
    shell: &str,
    script: &str,
    extra_args: &[String],
) -> io::Result<i32> {
    let shell = if shell.is_empty() {
        default_shell(&user.shell)
    } else {
        shell.to_string()
    };

    let mut command = if !script.is_empty() {
        let mut command = Command::new(&shell);
        command.arg("-c").arg(script);
        for arg in extra_args {
            command.arg(arg);
        }
        command
    } else if !extra_args.is_empty() {
        let mut command = Command::new(&extra_args[0]);
        for arg in &extra_args[1..] {
            command.arg(arg);
        }
        command
    } else {
        Command::new(&shell)
    };

    if login {
        command.arg0(format!("-{}", login_name(&shell)));
        command.env_clear();
        if let Ok(term) = env::var("TERM") {
            command.env("TERM", term);
        }
        command.env("PATH", DEFAULT_PATH);
    }

    if !preserve_env {
        command.env("HOME", &user.home);
        command.env("SHELL", &shell);
        command.env("USER", &user.name);
        command.env("LOGNAME", &user.name);
        command.env(
            "PATH",
            env::var("PATH").unwrap_or_else(|_| String::from(DEFAULT_PATH)),
        );
    }

    let child_user = user.clone();
    let home = user.home.clone();
    unsafe {
        command.pre_exec(move || {
            drop_privileges(&child_user)?;
            if login {
                env::set_current_dir(&home)?;
            }
            Ok(())
        });
    }

    command.status().map(exit_code)
}

pub(crate) fn sulogin_session(root: &SessionUser) -> io::Result<i32> {
    let shell = default_shell(&root.shell);
    let mut command = Command::new(&shell);
    command.arg0(format!("-{}", login_name(&shell)));
    command.env_clear();
    command.env("HOME", &root.home);
    command.env("SHELL", &shell);
    command.env("USER", "root");
    command.env("LOGNAME", "root");
    command.env(
        "PATH",
        env::var("PATH").unwrap_or_else(|_| String::from(DEFAULT_PATH)),
    );

    let child_user = root.clone();
    let home = root.home.clone();
    unsafe {
        command.pre_exec(move || {
            drop_privileges(&child_user)?;
            env::set_current_dir(&home)?;
            Ok(())
        });
    }

    command.status().map(exit_code)
}

fn parse_algorithm(value: &str) -> Result<Algorithm, String> {
    match value {
        "des" => Ok(Algorithm::Des),
        "md5" => Ok(Algorithm::Md5),
        "sha256" => Ok(Algorithm::Sha256),
        "sha512" => Ok(Algorithm::Sha512),
        _ => Err(format!("invalid algorithm '{value}'")),
    }
}

fn build_salt(algorithm: Algorithm) -> Result<String, String> {
    let prefix = match algorithm {
        Algorithm::Des => "",
        Algorithm::Md5 => "$1$",
        Algorithm::Sha256 => "$5$",
        Algorithm::Sha512 => "$6$",
    };
    let length = if matches!(algorithm, Algorithm::Des) {
        2
    } else {
        16
    };
    let body = random_salt(length)?;
    if matches!(algorithm, Algorithm::Des) {
        Ok(body)
    } else {
        Ok(format!("{prefix}{body}$"))
    }
}

fn random_salt(length: usize) -> Result<String, String> {
    const TABLE: &[u8] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

    let mut bytes = vec![0_u8; length];
    match File::open("/dev/urandom") {
        Ok(mut file) => file
            .read_exact(&mut bytes)
            .map_err(|err| format!("reading /dev/urandom: {err}"))?,
        Err(_) => {
            let seed = days_since_epoch() ^ u64::from(std::process::id());
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = seed.wrapping_add(index as u64) as u8;
            }
        }
    }

    Ok(bytes
        .into_iter()
        .map(|byte| TABLE[usize::from(byte) % TABLE.len()] as char)
        .collect())
}

fn days_since_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86_400
}

fn drop_privileges(user: &SessionUser) -> io::Result<()> {
    if rustix::process::geteuid().as_raw() == user.uid
        && rustix::process::getegid().as_raw() == user.gid
    {
        return Ok(());
    }

    let name = CString::new(user.name.as_bytes())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    #[cfg(target_os = "macos")]
    let initgroups_gid = user.gid as libc::c_int;
    #[cfg(not(target_os = "macos"))]
    let initgroups_gid = user.gid as libc::gid_t;

    unsafe {
        if libc::initgroups(name.as_ptr(), initgroups_gid) != 0 {
            return Err(io::Error::last_os_error());
        }

        if libc::setgid(user.gid as libc::gid_t) != 0 {
            return Err(io::Error::last_os_error());
        }

        if libc::setuid(user.uid as libc::uid_t) != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

fn exit_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

fn login_name(shell: &str) -> &str {
    shell.rsplit('/').next().unwrap_or(shell)
}

fn default_shell(shell: &str) -> String {
    if shell.is_empty() {
        String::from(DEFAULT_SHELL)
    } else {
        shell.to_string()
    }
}
