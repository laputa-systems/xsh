##! SELinux presence as libselinux decides it. No applet labels files, so each
##! uses this to tell the kernels where a context request is GNU's silent no-op
##! from the ones where it must be refused.
use gnu

## Whether the kernel lists selinuxfs, which is when libselinux treats SELinux
## as enabled whether or not the filesystem is mounted or enforcing.
export proc enabled() [fs, error] -> Result[Bool, Error] {
  Ok("selinuxfs" in fp"/proc/filesystems".read_text()?)
}

## Whether selinuxfs is mounted with its enforce switch present.
export proc mounted() [fs] -> Bool {
  fp"/sys/fs/selinux/enforce".exists() ?? false
}
