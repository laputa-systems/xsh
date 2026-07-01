use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

struct AuthFixture {
    root: TempDir,
    passwd: PathBuf,
    shadow: PathBuf,
    nologin: PathBuf,
}

impl AuthFixture {
    fn new() -> Self {
        let root = TempDir::new().expect("create auth fixture");
        Self {
            passwd: root.path().join("passwd"),
            shadow: root.path().join("shadow"),
            nologin: root.path().join("nologin.txt"),
            root,
        }
    }

    fn env(&self, command: &mut Command) {
        command
            .env("XSH_PASSWD_FILE", &self.passwd)
            .env("XSH_SHADOW_FILE", &self.shadow)
            .env("XSH_NOLOGIN_FILE", &self.nologin);
    }

    fn home(&self, name: &str) -> PathBuf {
        let home = self.root.path().join(name);
        fs::create_dir_all(&home).expect("create home");
        home
    }

    fn write_user(&self, name: &str, home: &Path, shell: &Path) {
        let (uid, gid) = current_ids();
        fs::write(
            &self.passwd,
            format!(
                "{name}:x:{uid}:{gid}:{name}:{}:{}\n",
                home.display(),
                shell.display()
            ),
        )
        .expect("write passwd");
        fs::write(&self.shadow, format!("{name}:hash:0:0:99999:7:::\n")).expect("write shadow");
    }

    fn write_users(&self, users: &[(&str, &Path, &Path)]) {
        let (uid, gid) = current_ids();
        let text = users
            .iter()
            .map(|(name, home, shell)| {
                format!(
                    "{name}:x:{uid}:{gid}:{name}:{}:{}",
                    home.display(),
                    shell.display()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(&self.passwd, text).expect("write passwd");
        let shadow = users
            .iter()
            .map(|(name, _, _)| format!("{name}:hash:0:0:99999:7:::"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(&self.shadow, shadow).expect("write shadow");
    }
}

fn current_ids() -> (u32, u32) {
    unsafe { (libc::geteuid() as u32, libc::getegid() as u32) }
}

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).expect("write helper script");
    let mut permissions = fs::metadata(&path)
        .expect("stat helper script")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("chmod helper script");
    path
}

fn run(command: &mut Command) -> Output {
    command.output().expect("run auth binary")
}

fn run_with_input(command: &mut Command, input: &str) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn auth binary");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait auth binary")
}

fn applet_command(name: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xsh"));
    command
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("core")
                .join(format!("{name}.xsh")),
        )
        .arg("--");
    command
}

fn auth_command(name: &str, fixture: &AuthFixture) -> Command {
    let mut command = applet_command(name);
    fixture.env(&mut command);
    command
}

fn shadow_password(path: &Path, user: &str) -> String {
    fs::read_to_string(path)
        .expect("read shadow")
        .lines()
        .find_map(|line| {
            let mut fields = line.split(':');
            (fields.next() == Some(user)).then(|| fields.next().unwrap_or("").to_string())
        })
        .expect("shadow entry")
}

#[test]
fn getty_no_prompt_hands_off_to_login_with_term() {
    let fixture = AuthFixture::new();
    let fake_login = script(
        fixture.root.path(),
        "fake-login",
        "#!/bin/sh\nprintf '%s:%s' \"${TERM-}\" \"$#\"\n",
    );
    let mut command = applet_command("getty");
    command.args([
        "-i",
        "-n",
        "-l",
        fake_login.to_str().unwrap(),
        "0",
        "/dev/null",
        "vt100",
    ]);

    let output = run(&mut command);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "vt100:0");
}

#[test]
fn getty_prompts_and_passes_username() {
    let fixture = AuthFixture::new();
    let fake_login = script(
        fixture.root.path(),
        "fake-login",
        "#!/bin/sh\nprintf '%s' \"$1\"\n",
    );
    let mut command = applet_command("getty");
    command.args(["-i", "-l", fake_login.to_str().unwrap(), "0", "/dev/null"]);

    let output = run_with_input(&mut command, "alice\n");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "login: alice");
}

#[test]
fn passwd_sets_default_password_hash() {
    let fixture = AuthFixture::new();
    let home = fixture.home("root-home");
    fixture.write_user("root", &home, Path::new("/bin/sh"));
    let mut command = auth_command("passwd", &fixture);
    command.arg("root");

    let output = run_with_input(&mut command, "secret\nsecret\n");

    assert!(output.status.success(), "{output:?}");
    let password = shadow_password(&fixture.shadow, "root");
    assert!(!password.is_empty());
    assert_ne!(password, "hash");
    #[cfg(target_os = "linux")]
    assert!(password.starts_with("$6$"), "{password}");
}

#[test]
fn passwd_honors_md5_algorithm() {
    let fixture = AuthFixture::new();
    let home = fixture.home("root-home");
    fixture.write_user("root", &home, Path::new("/bin/sh"));
    let mut command = auth_command("passwd", &fixture);
    command.args(["-a", "md5", "root"]);

    let output = run_with_input(&mut command, "secret\nsecret\n");

    assert!(output.status.success(), "{output:?}");
    let password = shadow_password(&fixture.shadow, "root");
    assert!(!password.is_empty());
    assert_ne!(password, "hash");
    #[cfg(target_os = "linux")]
    assert!(password.starts_with("$1$"), "{password}");
}

#[test]
fn passwd_delete_lock_and_unlock_update_shadow() {
    let fixture = AuthFixture::new();
    let home = fixture.home("root-home");
    fixture.write_user("root", &home, Path::new("/bin/sh"));

    let mut lock = auth_command("passwd", &fixture);
    lock.args(["-l", "root"]);
    assert!(run(&mut lock).status.success());
    assert_eq!(shadow_password(&fixture.shadow, "root"), "!hash");

    let mut unlock = auth_command("passwd", &fixture);
    unlock.args(["-u", "root"]);
    assert!(run(&mut unlock).status.success());
    assert_eq!(shadow_password(&fixture.shadow, "root"), "hash");

    let mut delete = auth_command("passwd", &fixture);
    delete.args(["-d", "root"]);
    assert!(run(&mut delete).status.success());
    assert_eq!(shadow_password(&fixture.shadow, "root"), "");
}

#[test]
fn passwd_rejects_mismatched_passwords_without_changing_shadow() {
    let fixture = AuthFixture::new();
    let home = fixture.home("root-home");
    fixture.write_user("root", &home, Path::new("/bin/sh"));
    let mut command = auth_command("passwd", &fixture);
    command.arg("root");

    let output = run_with_input(&mut command, "one\ntwo\n");

    assert!(!output.status.success(), "{output:?}");
    assert_eq!(shadow_password(&fixture.shadow, "root"), "hash");
}

#[cfg(target_os = "linux")]
#[test]
fn mdev_applies_rule_to_temp_device_tree() {
    let root = TempDir::new().expect("create mdev fixture");
    let dev_root = root.path().join("dev");
    let sysfs_root = root.path().join("sys");
    let device_dir = sysfs_root.join("devices/virtual/block/sda");
    let conf = root.path().join("mdev.conf");
    let log = root.path().join("mdev.log");
    fs::create_dir_all(&device_dir).expect("create sysfs device");
    fs::write(device_dir.join("dev"), "8:0\n").expect("write dev numbers");
    fs::write(device_dir.join("uevent"), "DEVNAME=sda\nSUBSYSTEM=block\n").expect("write uevent");
    let (uid, gid) = current_ids();
    fs::write(
        &conf,
        format!(
            "-SUBSYSTEM=block;(sd[a-z]) {uid}:{gid} 640 >disk/%1 @printf '%s:%s' \"$MDEV\" \"$ACTION\" > \"$LOG\"\n"
        ),
    )
    .expect("write mdev config");

    let mut add = applet_command("mdev");
    add.env("XSH_MDEV_DEV_ROOT", &dev_root)
        .env("XSH_MDEV_SYSFS", &sysfs_root)
        .env("XSH_MDEV_CONF", &conf)
        .env("XSH_MDEV_TEST_PLAIN_FILES", "1")
        .env("ACTION", "add")
        .env("DEVPATH", "/devices/virtual/block/sda")
        .env("DEVNAME", "sda")
        .env("SUBSYSTEM", "block")
        .env("LOG", &log);
    let add_output = run(&mut add);
    assert!(add_output.status.success(), "{add_output:?}");

    let node = dev_root.join("disk/sda");
    assert_eq!(fs::read_to_string(&node).expect("read node"), "b 8:0\n");
    assert_eq!(
        fs::metadata(&node).expect("stat node").permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(
        fs::read_link(dev_root.join("sda")).expect("read link"),
        PathBuf::from("disk/sda")
    );
    assert_eq!(
        fs::read_to_string(&log).expect("read command log"),
        "disk/sda:add"
    );

    let mut remove = applet_command("mdev");
    remove
        .env("XSH_MDEV_DEV_ROOT", &dev_root)
        .env("XSH_MDEV_SYSFS", &sysfs_root)
        .env("XSH_MDEV_CONF", &conf)
        .env("XSH_MDEV_TEST_PLAIN_FILES", "1")
        .env("ACTION", "remove")
        .env("DEVPATH", "/devices/virtual/block/sda")
        .env("DEVNAME", "sda")
        .env("SUBSYSTEM", "block")
        .env("LOG", &log);
    let remove_output = run(&mut remove);
    assert!(remove_output.status.success(), "{remove_output:?}");
    assert!(!node.exists());
    assert!(!dev_root.join("sda").exists());
}

#[test]
fn su_runs_command_as_target_user() {
    let fixture = AuthFixture::new();
    let home = fixture.home("nobody-home");
    fixture.write_user("nobody", &home, Path::new("/bin/sh"));
    let mut command = auth_command("su", &fixture);
    command.args(["-s", "/bin/sh", "nobody", "-c", "printf %s \"$USER\""]);

    let output = run(&mut command);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "nobody");
}

#[test]
fn su_login_mode_resets_home() {
    let fixture = AuthFixture::new();
    let home = fixture.home("root-home");
    fixture.write_user("root", &home, Path::new("/bin/sh"));
    let mut command = auth_command("su", &fixture);
    command
        .args(["-", "root", "-c", "printf %s \"$HOME\""])
        .env("HOME", "/tmp/original");

    let output = run(&mut command);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        home.display().to_string()
    );
}

#[test]
fn su_preserve_environment_keeps_home() {
    let fixture = AuthFixture::new();
    let home = fixture.home("root-home");
    fixture.write_user("root", &home, Path::new("/bin/sh"));
    let mut command = auth_command("su", &fixture);
    command
        .args(["-m", "root", "-c", "printf %s \"$HOME\""])
        .env("HOME", "/tmp/original");

    let output = run(&mut command);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "/tmp/original");
}

#[test]
fn passwd_defaults_to_current_user_from_xsh_passwd_file() {
    let fixture = AuthFixture::new();
    let home = fixture.home("current-home");
    let (uid, gid) = current_ids();
    fs::write(
        &fixture.passwd,
        format!("current:x:{uid}:{gid}:current:{}:/bin/sh\n", home.display()),
    )
    .expect("write passwd");
    fs::write(&fixture.shadow, "current:hash:0:0:99999:7:::\n").expect("write shadow");
    let mut command = auth_command("passwd", &fixture);
    command.arg("-d");

    let output = run(&mut command);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(shadow_password(&fixture.shadow, "current"), "");
}

#[test]
fn su_uses_xsh_passwd_file_for_current_user_match() {
    let fixture = AuthFixture::new();
    let shell = script(
        fixture.root.path(),
        "shell",
        "#!/bin/sh\nprintf '%s' \"$USER\"\n",
    );
    let first_home = fixture.home("first-home");
    let second_home = fixture.home("second-home");
    fixture.write_users(&[
        ("first", &first_home, &shell),
        ("second", &second_home, &shell),
    ]);
    let mut command = auth_command("su", &fixture);
    command.args(["first", "-c", "printf %s \"$USER\""]);

    let output = run(&mut command);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "first");
}
