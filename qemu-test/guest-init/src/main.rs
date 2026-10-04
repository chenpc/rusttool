//! Minimal static `/init` for the qemu-test guest.
//!
//! Deliberately dependency-free (std + libc only, no busybox). It:
//!   1. opens /dev/console so every message lands on the serial line,
//!   2. mounts proc + sysfs,
//!   3. loads the 9p modules (Alpine ships 9p as modules only), in dependency
//!      order, out of the initramfs copy in /modules,
//!   4. mounts a tmpfs on /tmp (payloads may want scratch space),
//!   5. reads rd.test=<id> from /proc/cmdline,
//!   6. mounts the read-only 9p share `hostshare` on /mnt,
//!   7. runs /mnt/tests/<id> (a static binary) in a forked child,
//!   8. prints `QEMU-TEST: PASS` or `QEMU-TEST: FAIL(<exit-or-signal>)`,
//!   9. powers the machine off through the reboot syscall.
//!
//! Every failure path (error, panic, fatal signal) still reports and powers
//! off, so the host harness never hangs waiting for a marker.

use std::ffi::CString;
use std::fmt;
use std::io;
use std::sync::atomic::{AtomicI32, Ordering};

/// 9p modules in dependency order. `netfs` is a module too on this kernel
/// (CONFIG_NETFS_SUPPORT=m), and 9pnet needs it, so it goes first.
///
/// Without these the share cannot be mounted and no testcase can run, so a
/// failure here fails the boot.
const REQUIRED_MODULES: [&str; 4] = [
    "/modules/netfs.ko",
    "/modules/9pnet.ko",
    "/modules/9p.ko",
    "/modules/9pnet_virtio.ko",
];

/// Block-device and filesystem modules, also in dependency order.
///
/// These only matter to the disk-oriented testcases (fdisk, partx, blkid,
/// mount, mkfs, ...), and the guest is booted with no `-drive` for everything
/// else, so a failure here is reported and then ignored: failing the boot would
/// take the other few dozen testcases down with it.
///
/// `crc16` and `mbcache` come before `ext4`, which needs both.
const OPTIONAL_MODULES: [&str; 8] = [
    "/modules/virtio_blk.ko",
    "/modules/loop.ko",
    "/modules/crc16.ko",
    "/modules/mbcache.ko",
    "/modules/jbd2.ko",
    "/modules/ext4.ko",
    "/modules/fat.ko",
    "/modules/vfat.ko",
];

const SHARE_TAG: &str = "hostshare";
const SHARE_MOUNTPOINT: &str = "/mnt";
const SHARE_OPTIONS: &str = "trans=virtio,version=9p2000.L,ro";

/// `LINUX_REBOOT_CMD_POWER_OFF`; spelled out (as its raw bit pattern) so the
/// crate does not depend on libc exporting it for the musl target.
const REBOOT_CMD_POWER_OFF: libc::c_int = 0x4321_fedc_u32 as libc::c_int;

/// Fatal signals that must still produce a log line and a poweroff.
const FATAL_SIGNALS: [libc::c_int; 6] = [
    libc::SIGSEGV,
    libc::SIGBUS,
    libc::SIGILL,
    libc::SIGFPE,
    libc::SIGABRT,
    libc::SIGTERM,
];

/// fd for /dev/console, or -1 until `init_console` succeeds.
static CONSOLE_FD: AtomicI32 = AtomicI32::new(-1);

fn console_fd() -> i32 {
    match CONSOLE_FD.load(Ordering::Relaxed) {
        fd if fd >= 0 => fd,
        _ => 2,
    }
}

fn raw_write(fd: i32, bytes: &[u8]) {
    let mut off = 0;
    while off < bytes.len() {
        let n = unsafe {
            libc::write(
                fd,
                bytes[off..].as_ptr() as *const libc::c_void,
                bytes.len() - off,
            )
        };
        if n <= 0 {
            break;
        }
        off += n as usize;
    }
}

fn cprint(args: fmt::Arguments<'_>) {
    let line = fmt::format(args);
    raw_write(console_fd(), line.as_bytes());
}

macro_rules! say {
    ($($arg:tt)*) => {
        cprint(format_args!($($arg)*))
    };
}

fn power_off() -> ! {
    // reboot(2) is the only "no ACPI tooling in the guest" poweroff path.
    // If the kernel refuses it we must not return into a half-booted guest:
    // the host harness has a timeout, so idle here and let it fire.
    loop {
        unsafe { libc::reboot(REBOOT_CMD_POWER_OFF) };
        unsafe { libc::pause() };
    }
}

extern "C" fn on_fatal_signal(sig: libc::c_int) {
    // Only async-signal-safe calls from here on: raw_write + reboot.
    let msg: &[u8] = match sig {
        libc::SIGSEGV => b"QEMU-INIT: fatal SIGSEGV\n",
        libc::SIGBUS => b"QEMU-INIT: fatal SIGBUS\n",
        libc::SIGILL => b"QEMU-INIT: fatal SIGILL\n",
        libc::SIGFPE => b"QEMU-INIT: fatal SIGFPE\n",
        libc::SIGABRT => b"QEMU-INIT: fatal SIGABRT\n",
        _ => b"QEMU-INIT: fatal SIGTERM\n",
    };
    raw_write(console_fd(), msg);
    power_off();
}

fn init_console() {
    let path = CString::new("/dev/console").expect("static path has no NUL");
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_WRONLY | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    };
    if fd >= 0 {
        CONSOLE_FD.store(fd, Ordering::Relaxed);
    }
}

fn install_signal_handlers() {
    for &sig in FATAL_SIGNALS.iter() {
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = on_fatal_signal as *const () as usize;
        action.sa_flags = libc::SA_RESTART;
        unsafe {
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(sig, &action, std::ptr::null_mut());
        }
    }
}

fn cstring(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|_| format!("string contains NUL: {:?}", s))
}

fn mount(
    source: &str,
    target: &str,
    fstype: &str,
    flags: libc::c_ulong,
    data: &str,
) -> Result<(), String> {
    let c_source = cstring(source)?;
    let c_target = cstring(target)?;
    let c_fstype = cstring(fstype)?;
    let c_data = cstring(data)?;
    let rc = unsafe {
        libc::mount(
            c_source.as_ptr(),
            c_target.as_ptr(),
            c_fstype.as_ptr(),
            flags,
            c_data.as_ptr() as *const libc::c_void,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "mount -t {} {} on {} ({}): {}",
            fstype,
            source,
            target,
            data,
            io::Error::last_os_error()
        ))
    }
}

/// Number of bytes handed to the kernel as the module-init info block. Linux
/// 6.13 changed `init_module`/`finit_module` to take a pointer to a
/// `struct module_init_info` instead of a params string, and returns EFAULT
/// when that pointer is NULL. A zero-filled block of this size encodes "no
/// flags, no params" for any sane field order, so it is the first thing we
/// try; the legacy NULL/empty-string encodings follow as fallbacks.
const INIT_INFO_LEN: usize = 32;

fn try_init_module(image: &[u8], info_arg: *const libc::c_void) -> Option<io::Error> {
    let rc = unsafe {
        libc::syscall(
            libc::SYS_init_module,
            image.as_ptr() as *const libc::c_void,
            image.len(),
            info_arg,
        )
    };
    if rc == 0 {
        None
    } else {
        Some(io::Error::last_os_error())
    }
}

fn try_finit_module(fd: libc::c_int, info_arg: *const libc::c_void) -> Option<io::Error> {
    let rc = unsafe { libc::syscall(libc::SYS_finit_module, fd, info_arg, 0 as libc::c_uint) };
    if rc == 0 {
        None
    } else {
        Some(io::Error::last_os_error())
    }
}

fn load_module(path: &str) -> Result<(), String> {
    // libc does not expose init_module/finit_module for the musl target, so
    // issue the syscalls directly.
    let image = std::fs::read(path).map_err(|e| format!("read {}: {}", path, e))?;

    let init_info = [0u8; INIT_INFO_LEN];
    let mut problems: Vec<String> = Vec::new();

    // 1. current ABI: struct module_init_info { u64 flags; u64 len; char *params; }
    match try_init_module(&image, init_info.as_ptr() as *const libc::c_void) {
        None => return Ok(()),
        Some(err) => problems.push(format!("init_module(struct): {}", err)),
    }

    // 2. legacy ABI: plain params string, here empty.
    match try_init_module(&image, b"\0".as_ptr() as *const libc::c_void) {
        None => return Ok(()),
        Some(err) => problems.push(format!("init_module(params): {}", err)),
    }

    // 3. file-descriptor flavour, same two encodings.
    let cpath = cstring(path)?;
    let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
    if fd >= 0 {
        if let Some(err) = try_finit_module(fd, init_info.as_ptr() as *const libc::c_void) {
            problems.push(format!("finit_module(struct): {}", err));
        }
        if let Some(err) = try_finit_module(fd, b"\0".as_ptr() as *const libc::c_void) {
            problems.push(format!("finit_module(params): {}", err));
        }
        unsafe { libc::close(fd) };
    } else {
        problems.push(format!("open: {}", io::Error::last_os_error()));
    }

    Err(format!("load {}: {}", path, problems.join("; ")))
}

fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && !id.contains('/')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Run the init stage scripts from the share before anything else.
///
/// `/mnt/init.d/*.sh` in name order, each executed with brush and the same
/// environment as a testcase (`PATH=/mnt/bin`). `/init` stays a plain
/// bring-up program; boot policy lives in scripts that need no initramfs
/// repack. The first failure aborts the boot with FAIL.
fn run_stage_scripts() -> Result<(), String> {
    let dir = format!("{}/init.d", SHARE_MOUNTPOINT);
    let entries = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".sh"))
            .collect::<Vec<String>>(),
        Err(e) => return Err(format!("read_dir {}: {}", dir, e)),
    };
    if entries.is_empty() {
        say!("QEMU-INIT: stage-script-none\n");
        return Ok(());
    }
    let mut names = entries;
    names.sort();
    for name in names {
        let path = format!("{}/{}", dir, name);
        say!("QEMU-INIT: stage-script-start {}\n", name);
        match run_shell_tty(Some(&path)) {
            Ok(()) => say!("QEMU-INIT: stage-script-ok {}\n", name),
            Err(reason) => {
                say!("QEMU-INIT: stage-script-err {} {}\n", name, reason);
                return Err(format!("stage {}: {}", name, reason));
            }
        }
    }
    Ok(())
}

/// What the kernel command line asked for.
enum Mode {
    /// `rd.test=<id>`: run that one payload from the share.
    Test(String),
    /// `rd.shell`, or no mode token at all: interactive brush on a pty inside
    /// the guest, with the console proxied in — prompt, line editing and
    /// history all work. This is the default, so a bare boot is a usable shell.
    Shell,
    /// `rd.shellpipe`: brush reading from a pipe fed by the console. No
    /// prompt and no line editing, but nothing can stall; the fallback when
    /// the pty path misbehaves.
    ShellPipe,
    /// `rd.shelltty`: brush straight on the console tty. Needs a host pty that
    /// answers cursor-position queries; otherwise brush gives up.
    ShellTty,
    /// `rd.shellscript`: run `/mnt/tests/.shellcmd` through brush. Used by
    /// run-shell.sh for a scripted, non-interactive guest shell.
    ShellScript,
}

fn parse_mode() -> Result<Mode, String> {
    let cmdline =
        std::fs::read_to_string("/proc/cmdline").map_err(|e| format!("read /proc/cmdline: {}", e))?;
    let mut test: Option<String> = None;
    let mut shell = false;
    let mut shell_pipe = false;
    let mut shell_tty = false;
    let mut shell_script = false;
    for token in cmdline.split_whitespace() {
        if let Some(id) = token.strip_prefix("rd.test=") {
            if !is_safe_id(id) {
                return Err(format!("unsafe rd.test={:?}", id));
            }
            test = Some(id.to_string());
        } else if token == "rd.shell" {
            shell = true;
        } else if token == "rd.shellpipe" {
            shell_pipe = true;
        } else if token == "rd.shelltty" {
            shell_tty = true;
        } else if token == "rd.shellscript" {
            shell_script = true;
        }
    }
    if let Some(id) = test {
        return Ok(Mode::Test(id));
    }
    if shell_script {
        return Ok(Mode::ShellScript);
    }
    if shell_pipe {
        return Ok(Mode::ShellPipe);
    }
    if shell_tty {
        return Ok(Mode::ShellTty);
    }
    let _ = shell;
    Ok(Mode::Shell)
}

/// fork/exec the payload; `Err` carries the `<exit-or-signal>` text used in
/// the FAIL marker.
///
/// Payload resolution: `/mnt/tests/<id>` (a static binary) is preferred;
/// when it does not exist and `/mnt/tests/<id>.sh` does, the test is a shell
/// script executed by brush (`/mnt/bin/brush`, shipped on the 9p share).
/// Both flavours run with `PATH=/mnt/bin` so tests can invoke the tools
/// under test by name, and writable scratch rooted at `/tmp`.
const BRUSH_PATH: &str = "/mnt/bin/brush";
const PAYLOAD_ENV: [&str; 3] = ["PATH=/mnt/bin", "HOME=/tmp", "TMPDIR=/tmp"];

fn exists(path: &str) -> bool {
    match cstring(path) {
        Ok(cpath) => unsafe { libc::access(cpath.as_ptr(), libc::F_OK) == 0 },
        Err(_) => false,
    }
}

fn run_test(id: &str) -> Result<(), String> {
    let binary = format!("{}/tests/{}", SHARE_MOUNTPOINT, id);
    let script = format!("{}.sh", binary);

    // Decide (and build every CString) before fork: the child must not
    // allocate-then-panic between fork and exec.
    let (program, args): (String, Vec<String>) = if exists(&binary) {
        (binary.clone(), vec![binary])
    } else if exists(&script) {
        (BRUSH_PATH.to_string(), vec![BRUSH_PATH.to_string(), script])
    } else {
        return Err(format!("missing test {}", id));
    };

    let c_program = cstring(&program)?;
    let c_args: Vec<CString> = args
        .iter()
        .map(|a| cstring(a))
        .collect::<Result<_, _>>()?;
    let c_env: Vec<CString> = PAYLOAD_ENV
        .iter()
        .map(|kv| cstring(kv))
        .collect::<Result<_, _>>()?;

    let mut argv: Vec<*const libc::c_char> =
        c_args.iter().map(|a| a.as_ptr()).collect();
    argv.push(std::ptr::null());
    let mut envp: Vec<*const libc::c_char> =
        c_env.iter().map(|e| e.as_ptr()).collect();
    envp.push(std::ptr::null());

    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(format!("fork: {}", io::Error::last_os_error()));
    }
    if pid == 0 {
        // Child: exec; report straight to the console and _exit on failure.
        unsafe {
            libc::execve(
                c_program.as_ptr(),
                argv.as_ptr(),
                envp.as_ptr(),
            );
            let msg = b"QEMU-INIT: execve failed\n";
            libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
            libc::_exit(127);
        }
    }

    let mut status: libc::c_int = 0;
    loop {
        let r = unsafe { libc::waitpid(pid, &mut status, 0) };
        if r == pid {
            break;
        }
        if r < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        if r < 0 {
            return Err(format!("waitpid: {}", io::Error::last_os_error()));
        }
    }

    if libc::WIFEXITED(status) {
        let code = libc::WEXITSTATUS(status);
        if code == 0 {
            Ok(())
        } else {
            Err(format!("exit={}", code))
        }
    } else if libc::WIFSIGNALED(status) {
        Err(format!("signal={}", libc::WTERMSIG(status)))
    } else {
        Err(format!("status=0x{:x}", status))
    }
}

/// Wait for a child and map its status to Ok/Err like run_test does.
fn wait_child(pid: libc::pid_t) -> Result<(), String> {
    let mut status: libc::c_int = 0;
    loop {
        let r = unsafe { libc::waitpid(pid, &mut status, 0) };
        if r == pid {
            break;
        }
        if r < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        if r < 0 {
            return Err(format!("waitpid: {}", io::Error::last_os_error()));
        }
    }
    if libc::WIFEXITED(status) {
        let code = libc::WEXITSTATUS(status);
        if code == 0 {
            Ok(())
        } else {
            Err(format!("exit={}", code))
        }
    } else if libc::WIFSIGNALED(status) {
        Err(format!("signal={}", libc::WTERMSIG(status)))
    } else {
        Err(format!("status=0x{:x}", status))
    }
}

/// Run brush on the serial console. `script` is `Some(path)` for a script on
/// the share, or `None` for a session fed by the console.
///
/// The child turns /dev/console into its stdio. PID 1 is already a session
/// leader without a controlling terminal, so opening /dev/console without
/// O_NOCTTY acquires it as the controlling tty.
fn run_shell_tty(script: Option<&str>) -> Result<(), String> {
    if !exists(BRUSH_PATH) {
        return Err(format!("missing {} on the share", BRUSH_PATH));
    }
    let console = cstring("/dev/console")?;
    let c_program = cstring(BRUSH_PATH)?;
    let script_arg = match script {
        Some(path) => Some(cstring(path)?),
        None => None,
    };
    // TERM=dumb: the guest has no terminfo database, and dumb needs none.
    let env: [&str; 4] = ["PATH=/mnt/bin", "HOME=/tmp", "TMPDIR=/tmp", "TERM=dumb"];
    let c_env: Vec<CString> = env.iter().map(|kv| cstring(kv)).collect::<Result<_, _>>()?;

    let mut argv: Vec<*const libc::c_char> = vec![c_program.as_ptr()];
    if let Some(s) = &script_arg {
        argv.push(s.as_ptr());
    }
    argv.push(std::ptr::null());
    let mut envp: Vec<*const libc::c_char> = c_env.iter().map(|e| e.as_ptr()).collect();
    envp.push(std::ptr::null());

    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(format!("fork: {}", io::Error::last_os_error()));
    }
    if pid == 0 {
        unsafe {
            let fd = libc::open(
                console.as_ptr(),
                libc::O_RDWR | libc::O_CLOEXEC,
            );
            if fd < 0 {
                let msg = b"QEMU-INIT: open /dev/console failed\n";
                libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
                libc::_exit(127);
            }
            libc::dup2(fd, 0);
            libc::dup2(fd, 1);
            libc::dup2(fd, 2);
            if fd > 2 {
                libc::close(fd);
            }
            libc::execve(c_program.as_ptr(), argv.as_ptr(), envp.as_ptr());
            let msg = b"QEMU-INIT: execve brush failed\n";
            libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
            libc::_exit(127);
        }
    }
    wait_child(pid)
}

/// Interactive-ish shell: brush reads commands from a **pipe** that a small
/// feeder process keeps filled from the serial console.
///
/// Why not give brush the console tty directly: its line editor (reedline)
/// queries the terminal with `ESC[6n` for the cursor position and aborts when
/// nothing answers ("The cursor position could not be read within a normal
/// duration") — and a serial line has no terminal emulator on the far end to
/// reply. With a pipe on stdin brush skips the interactive line editor and
/// reads commands line by line, which works on any console.
///
/// Line editing (arrows, history) is therefore unavailable in this mode; use
/// `rd.shelltty` when the host side really is a pty that answers DSR.
fn run_shell_piped() -> Result<(), String> {
    if !exists(BRUSH_PATH) {
        return Err(format!("missing {} on the share", BRUSH_PATH));
    }
    let console = cstring("/dev/console")?;
    let c_program = cstring(BRUSH_PATH)?;
    let env: [&str; 4] = ["PATH=/mnt/bin", "HOME=/tmp", "TMPDIR=/tmp", "TERM=dumb"];
    let c_env: Vec<CString> = env.iter().map(|kv| cstring(kv)).collect::<Result<_, _>>()?;
    let mut argv: Vec<*const libc::c_char> = vec![c_program.as_ptr(), std::ptr::null()];
    let mut envp: Vec<*const libc::c_char> = c_env.iter().map(|e| e.as_ptr()).collect();
    envp.push(std::ptr::null());

    let console_fd = unsafe {
        libc::open(console.as_ptr(), libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC)
    };
    if console_fd < 0 {
        return Err(format!("open /dev/console: {}", io::Error::last_os_error()));
    }

    let mut ends: [libc::c_int; 2] = [0; 2];
    if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
        return Err(format!("pipe: {}", io::Error::last_os_error()));
    }
    let (read_end, write_end) = (ends[0], ends[1]);

    // Feeder: copy the console into the pipe until EOF (when brush exits and
    // the last write end is closed).
    let feeder = unsafe { libc::fork() };
    if feeder < 0 {
        return Err(format!("fork feeder: {}", io::Error::last_os_error()));
    }
    if feeder == 0 {
        let mut buf = [0u8; 4096];
        unsafe {
            libc::close(read_end);
            libc::dup2(console_fd, 0);
            loop {
                let n = libc::read(0, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
                if n <= 0 {
                    break;
                }
                let mut off = 0isize;
                while off < n as isize {
                    let w = libc::write(
                        write_end,
                        buf.as_ptr().offset(off) as *const libc::c_void,
                        (n as isize - off) as usize,
                    );
                    if w <= 0 {
                        libc::_exit(0);
                    }
                    off += w as isize;
                }
            }
            libc::_exit(0);
        }
    }

    // Shell: stdin from the pipe, stdout/stderr on the console.
    let shell = unsafe { libc::fork() };
    if shell < 0 {
        return Err(format!("fork shell: {}", io::Error::last_os_error()));
    }
    if shell == 0 {
        unsafe {
            libc::close(write_end);
            libc::dup2(read_end, 0);
            libc::dup2(console_fd, 1);
            libc::dup2(console_fd, 2);
            if console_fd > 2 {
                libc::close(console_fd);
            }
            libc::execve(c_program.as_ptr(), argv.as_ptr(), envp.as_ptr());
            let msg = b"QEMU-INIT: execve brush failed\n";
            libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
            libc::_exit(127);
        }
    }

    // Parent: drop both pipe ends so the feeder sees EOF once brush is gone.
    unsafe {
        libc::close(read_end);
        libc::close(write_end);
    }
    let outcome = wait_child(shell);
    // The feeder is still blocked reading the console tty, which never reaches
    // EOF -- waiting for it would hang the boot. Kill and reap it instead.
    let mut status: libc::c_int = 0;
    unsafe {
        libc::kill(feeder, libc::SIGKILL);
        libc::waitpid(feeder, &mut status, 0);
    }
    outcome
}

/// A pty-based interactive brush, with a proxy that impersonates the terminal
/// on the far end of the pty.
///
/// Two problems this solves, both found by running brush over a serial line:
///
/// 1. brush's line editor (reedline) queries the terminal with `ESC[6n` for
///    the cursor position and aborts when nothing answers within a timeout
///    ("The cursor position could not be read within a normal duration").
///    A serial console has no terminal emulator behind it, so [`QueryFilter`]
///    answers those queries itself.
/// 2. With a pipe on stdin brush is not interactive, so it prints no `PS1`.
///    On a pty it is genuinely interactive: prompt, line editing, history and
///    command completion all work.
///
/// The proxy forwards console input into the pty master and pty output back to
/// the console; when brush exits, the master reports EOF and the loop ends.
/// Put a terminal into raw-ish mode before the shell starts.
///
/// The important bit is ICANON: in canonical mode the tty withholds input
/// until a newline arrives, so the `ESC[1;1R` cursor-position reply our proxy
/// injects would sit in the buffer forever and the line editor would keep
/// waiting for it. ECHO goes away too because the line editor renders the line
/// itself; ISIG stays so Ctrl-C still interrupts.
fn make_raw(fd: libc::c_int) -> Result<(), String> {
    unsafe {
        let mut tio: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut tio) != 0 {
            return Err(format!("tcgetattr: {}", io::Error::last_os_error()));
        }
        tio.c_lflag &= !(libc::ICANON | libc::ECHO);
        tio.c_iflag &= !(libc::ICRNL | libc::INLCR | libc::IGNCR);
        tio.c_cc[libc::VMIN] = 1;
        tio.c_cc[libc::VTIME] = 0;
        if libc::tcsetattr(fd, libc::TCSANOW, &tio) != 0 {
            return Err(format!("tcsetattr: {}", io::Error::last_os_error()));
        }
    }
    Ok(())
}

fn run_shell_pty() -> Result<(), String> {
    if !exists(BRUSH_PATH) {
        return Err(format!("missing {} on the share", BRUSH_PATH));
    }

    // A pty for the shell: master stays here (proxy side), slave becomes the
    // shell's stdio.
    let master = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
    if master < 0 {
        return Err(format!("posix_openpt: {}", io::Error::last_os_error()));
    }
    if unsafe { libc::grantpt(master) } != 0 {
        return Err(format!("grantpt: {}", io::Error::last_os_error()));
    }
    if unsafe { libc::unlockpt(master) } != 0 {
        return Err(format!("unlockpt: {}", io::Error::last_os_error()));
    }
    let slave_name = unsafe {
        let raw = libc::ptsname(master);
        if raw.is_null() {
            return Err(format!("ptsname: {}", io::Error::last_os_error()));
        }
        std::ffi::CStr::from_ptr(raw).to_owned()
    };
    let slave = unsafe { libc::open(slave_name.as_ptr(), libc::O_RDWR | libc::O_NOCTTY) };
    if slave < 0 {
        return Err(format!("open {}: {}", slave_name.to_string_lossy(), io::Error::last_os_error()));
    }
    // 80x24 so the line editor has a sane window size.
    let ws = libc::winsize { ws_row: 24, ws_col: 80, ws_xpixel: 0, ws_ypixel: 0 };
    unsafe {
        libc::ioctl(master, libc::TIOCSWINSZ, &ws as *const libc::winsize);
    }
    // Raw before the shell starts, so cursor-position replies are delivered
    // immediately (see make_raw).
    make_raw(slave)?;

    let console = cstring("/dev/console")?;
    let console_in = unsafe {
        libc::open(console.as_ptr(), libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC)
    };
    if console_in < 0 {
        return Err(format!("open /dev/console: {}", io::Error::last_os_error()));
    }
    // The console goes raw too: ICRNL would otherwise rewrite the CR that
    // Enter sends into a plain LF, which the line editor does not accept, and
    // ECHO would double every keystroke next to the line editor's own output.
    make_raw(console_in)?;
    // ... and OPOST off, so the CRLFs the shell already emits are not doubled
    // into CRCRLF by the console's own ONLCR.
    unsafe {
        let mut tio: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(console_in, &mut tio) == 0 {
            tio.c_oflag &= !libc::OPOST;
            libc::tcsetattr(console_in, libc::TCSANOW, &tio);
        }
    }

    let c_program = cstring(BRUSH_PATH)?;
    // TERM + TERMINFO: the image ships no terminfo database, but the line
    // editor needs one once stdin is a pty. pack-initramfs.sh copies a few
    // profiles from the host into the share.
    let env: [&str; 5] = [
        "PATH=/mnt/bin",
        "HOME=/tmp",
        "TMPDIR=/tmp",
        "TERM=xterm",
        "TERMINFO=/mnt/etc/terminfo",
    ];
    let c_env: Vec<CString> = env.iter().map(|kv| cstring(kv)).collect::<Result<_, _>>()?;

    // An rcfile on the share sets the prompt; without it brush uses its own.
    // Every CString pushed into argv must outlive the exec, so bind them to
    // locals instead of pushing temporaries.
    let mut argv: Vec<*const libc::c_char> = vec![c_program.as_ptr()];
    let rc_path = format!("{}/etc/brushrc", SHARE_MOUNTPOINT);
    let rc_flag = cstring("--rcfile")?;
    let rc_c = if exists(&rc_path) { Some(cstring(&rc_path)?) } else { None };
    if let Some(rc) = &rc_c {
        argv.push(rc_flag.as_ptr());
        argv.push(rc.as_ptr());
    }
    // The default `reedline` backend asks the terminal for the cursor position
    // (ESC[6n) and gives up when nothing answers -- and a serial line has no
    // terminal emulator behind it. `basic` reads lines without any of that,
    // while still printing PS1 and handling the line editing basics.
    let backend_flag = cstring("--input-backend")?;
    let backend_basic = cstring("basic")?;
    argv.push(backend_flag.as_ptr());
    argv.push(backend_basic.as_ptr());
    // Interactive explicitly: stdin is a pty, but do not rely on brush's own
    // tty detection to decide that it should print a prompt.
    let interactive = cstring("-i")?;
    argv.push(interactive.as_ptr());
    // Bracketed paste turns into escape sequences nobody can paste usefully,
    // and colour codes are just noise on a serial line.
    let no_paste = cstring("--disable-bracketed-paste")?;
    argv.push(no_paste.as_ptr());
    let no_color = cstring("--disable-color")?;
    argv.push(no_color.as_ptr());
    argv.push(std::ptr::null());
    let mut envp: Vec<*const libc::c_char> = c_env.iter().map(|e| e.as_ptr()).collect();
    envp.push(std::ptr::null());

    let shell = unsafe { libc::fork() };
    if shell < 0 {
        return Err(format!("fork: {}", io::Error::last_os_error()));
    }
    if shell == 0 {
        unsafe {
            libc::close(master);
            libc::setsid();
            // Acquire the pty as our controlling terminal.
            libc::ioctl(slave, libc::TIOCSCTTY, 0);
            libc::dup2(slave, 0);
            libc::dup2(slave, 1);
            libc::dup2(slave, 2);
            if slave > 2 {
                libc::close(slave);
            }
            if console_in > 2 {
                libc::close(console_in);
            }
            // Start in the writable scratch dir.
            libc::chdir(cstring("/tmp")?.as_ptr());
            libc::execve(c_program.as_ptr(), argv.as_ptr(), envp.as_ptr());
            let msg = b"QEMU-INIT: execve brush failed\n";
            libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
            libc::_exit(127);
        }
    }

    // Proxy side: keep console_in (O_RDWR) -- it is both where the shell's
    // output goes and where the user's keystrokes come from. CONSOLE_FD is
    // O_WRONLY and would never report readable to poll().
    unsafe {
        libc::close(slave);
    }
    let mut filter = QueryFilter::new();
    let mut buf = [0u8; 4096];
    loop {
        let mut fds = [
            libc::pollfd { fd: master, events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: console_in, events: libc::POLLIN, revents: 0 },
        ];
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
        if rc < 0 {
            if io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }
        if fds[0].revents & (libc::POLLIN | libc::POLLHUP) != 0 {
            let n = unsafe { libc::read(master, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n <= 0 {
                break; // brush closed the pty: session over
            }
            for &byte in &buf[..n as usize] {
                let (reply, forward) = filter.feed(byte);
                if !forward.is_empty() {
                    // Writes go through CONSOLE_FD (the O_WRONLY fd opened by
                    // init_console): writes to the O_RDWR console fd are
                    // silently dropped in this guest, while reads from it work.
                    raw_write_all(console_fd(), &forward);
                }
                if let Some(reply) = reply {
                    raw_write_all(master, reply);
                }
            }
        }
        if fds[1].revents & libc::POLLIN != 0 {
            let n = unsafe {
                libc::read(console_in, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
            };
            if n < 0 {
                continue;
            }
            raw_write_all(master, &buf[..n as usize]);
        }
    }

    let outcome = wait_child(shell);
    unsafe {
        libc::close(master);
    }
    outcome
}

/// write(2) that retries short writes; used by the pty proxy.
fn raw_write_all(fd: i32, bytes: &[u8]) {
    let mut off = 0;
    while off < bytes.len() {
        let n = unsafe {
            libc::write(fd, bytes[off..].as_ptr() as *const libc::c_void, bytes.len() - off)
        };
        if n <= 0 {
            if n < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }
        off += n as usize;
    }
}

/// Recognises the terminal queries a line editor sends and produces the
/// replies a terminal emulator would send, so the editor does not stall.
///
/// Handles `ESC [ 6 n` (cursor position report, also `ESC [ ? 6 n`) and
/// `ESC [ c` (primary device attributes). Bytes that turn out not to be a
/// query are passed through, so ordinary escape sequences still reach the
/// console.
struct QueryFilter {
    state: u8,
    pending: Vec<u8>,
}

impl QueryFilter {
    const CPR: &'static [u8] = b"\x1b[1;1R";
    const DA1: &'static [u8] = b"\x1b[?1;2c";

    fn new() -> QueryFilter {
        QueryFilter { state: 0, pending: Vec::new() }
    }

    /// Feed one byte from the pty. Returns `(reply for the pty, bytes to show)`.
    fn feed(&mut self, byte: u8) -> (Option<&'static [u8]>, Vec<u8>) {
        let before = self.state;
        match before {
            0 => {
                if byte == 0x1b {
                    // Might be a terminal query: hold the ESC until we know.
                    self.state = 1;
                    self.pending.push(byte);
                    return (None, Vec::new());
                }
                return (None, vec![byte]);
            }
            1 => {
                if byte == b'[' {
                    self.state = 2;
                    self.pending.push(byte);
                    return (None, Vec::new());
                }
                // Not a CSI: flush what we held on to.
                let mut out = std::mem::take(&mut self.pending);
                out.push(byte);
                self.state = if byte == 0x1b { 1 } else { 0 };
                if self.state == 1 {
                    self.pending.push(byte);
                }
                return (None, out);
            }
            _ => {}
        }
        self.pending.push(byte);
        let next = match (before, byte) {
            (2, b'6') => 3,                    // ESC [ 6  -> expect 'n'
            (2, b'?') => 4,                    // ESC [ ?  -> private query
            (3, b'n') => 0,                    // done
            (4, b'6') => 5,
            (4, b'c') => 6,
            (5, b'n') => 0,
            (6, b'c') => 0,
            _ => 0,
        };
        let matched = next == 0
            && matches!(
                (before, byte),
                (3, b'n') | (5, b'n') | (6, b'c')
            );
        self.state = next;
        if matched {
            let reply = match byte {
                b'c' => Self::DA1,
                _ => Self::CPR,
            };
            self.pending.clear();
            (Some(reply), Vec::new())
        } else if self.state == 0 {
            let out = std::mem::take(&mut self.pending);
            (None, out)
        } else {
            (None, Vec::new())
        }
    }
}

fn real_main() -> Result<(), String> {
    say!("QEMU-INIT: start\n");

    // /dev before anything else: the console node and the shell's controlling
    // tty both live here, and devtmpfs supplies them without shipping device
    // nodes in the initramfs. Non-fatal: kernels built without DEVTMPFS still
    // run the payload tests (messages fall back to fd 2).
    match mount("devtmpfs", "/dev", "devtmpfs", 0, "") {
        Ok(()) => {}
        Err(reason) => say!("QEMU-INIT: stage=devtmpfs-skip {}\n", reason),
    }
    // Now that /dev/console exists, (re)open it: every later message and the
    // shell's stdio go through a real tty node instead of the inherited fd 2.
    init_console();

    // devpts, so pseudo terminals can be allocated: the interactive shell
    // gives brush a pty, and tests may want one too. Opening /dev/ptmx fails
    // with ENOENT until devpts is mounted, and /dev/pts is not always present
    // on devtmpfs.
    if std::fs::create_dir_all("/dev/pts").is_err() {
        say!("QEMU-INIT: stage=devpts-dir-failed\n");
    }
    match mount("devpts", "/dev/pts", "devpts", 0, "mode=0620") {
        Ok(()) => say!("QEMU-INIT: stage=devpts-ok\n"),
        Err(reason) => say!("QEMU-INIT: stage=devpts-skip {}\n", reason),
    }

    mount("proc", "/proc", "proc", 0, "")?;
    say!("QEMU-INIT: stage=proc-ok\n");

    mount("sysfs", "/sys", "sysfs", 0, "")?;
    say!("QEMU-INIT: stage=sysfs-ok\n");

    // Try every module and report each attempt: one boot then tells us whether
    // a failure is order-related, a vermagic problem, or a single bad module.
    let mut module_error: Option<String> = None;
    for module in REQUIRED_MODULES.iter() {
        match load_module(module) {
            Ok(()) => say!("QEMU-INIT: stage=module-ok {}\n", module),
            Err(reason) => {
                say!("QEMU-INIT: stage=module-err {} {}\n", module, reason);
                if module_error.is_none() {
                    module_error = Some(reason);
                }
            }
        }
    }
    if let Some(reason) = module_error {
        return Err(reason);
    }

    // The disk modules are a convenience, not a prerequisite: the share is
    // already up and the testcases that do not touch a disk are unaffected.
    let mut optional_error: Option<String> = None;
    for module in OPTIONAL_MODULES.iter() {
        match load_module(module) {
            Ok(()) => say!("QEMU-INIT: stage=module-opt-ok {}\n", module),
            Err(reason) => {
                say!("QEMU-INIT: stage=module-opt-err {} {}\n", module, reason);
                if optional_error.is_none() {
                    optional_error = Some(reason);
                }
            }
        }
    }
    if optional_error.is_some() {
        say!("QEMU-INIT: stage=module-optional-partial\n");
    }

    mount("tmpfs", "/tmp", "tmpfs", 0, "mode=1777")?;
    say!("QEMU-INIT: stage=tmpfs-ok\n");

    let mode = parse_mode()?;
    match &mode {
        Mode::Test(id) => say!("QEMU-INIT: stage=cmdline-ok rd.test={}\n", id),
        Mode::Shell => say!("QEMU-INIT: stage=cmdline-ok rd.shell\n"),
        Mode::ShellPipe => say!("QEMU-INIT: stage=cmdline-ok rd.shellpipe\n"),
        Mode::ShellTty => say!("QEMU-INIT: stage=cmdline-ok rd.shelltty\n"),
        Mode::ShellScript => say!("QEMU-INIT: stage=cmdline-ok rd.shellscript\n"),
    }

    mount(
        SHARE_TAG,
        SHARE_MOUNTPOINT,
        "9p",
        libc::MS_RDONLY,
        SHARE_OPTIONS,
    )?;
    say!("QEMU-INIT: stage=9p-ok {}\n", SHARE_MOUNTPOINT);

    // Boot policy lives on the share, not in the initramfs.
    run_stage_scripts()?;

    let outcome = match &mode {
        Mode::Test(id) => {
            let outcome = run_test(id);
            say!(
                "QEMU-INIT: stage=test-done id={} outcome={}\n",
                id,
                match &outcome {
                    Ok(()) => "ok".to_string(),
                    Err(reason) => reason.clone(),
                }
            );
            outcome
        }
        Mode::Shell => {
            say!("QEMU-INIT: stage=shell-start pty\n");
            let outcome = run_shell_pty();
            say!(
                "QEMU-INIT: stage=shell-done outcome={}\n",
                match &outcome {
                    Ok(()) => "ok".to_string(),
                    Err(reason) => reason.clone(),
                }
            );
            outcome
        }
        Mode::ShellPipe => {
            say!("QEMU-INIT: stage=shell-start piped\n");
            let outcome = run_shell_piped();
            say!(
                "QEMU-INIT: stage=shell-done outcome={}\n",
                match &outcome {
                    Ok(()) => "ok".to_string(),
                    Err(reason) => reason.clone(),
                }
            );
            outcome
        }
        Mode::ShellTty => {
            say!("QEMU-INIT: stage=shell-start tty\n");
            let outcome = run_shell_tty(None);
            say!(
                "QEMU-INIT: stage=shell-done outcome={}\n",
                match &outcome {
                    Ok(()) => "ok".to_string(),
                    Err(reason) => reason.clone(),
                }
            );
            outcome
        }
        Mode::ShellScript => {
            let script = format!("{}/tests/.shellcmd", SHARE_MOUNTPOINT);
            if !exists(&script) {
                return Err(format!("missing {} on the share", script));
            }
            say!("QEMU-INIT: stage=shell-start script={}\n", script);
            let outcome = run_shell_tty(Some(&script));
            say!(
                "QEMU-INIT: stage=shell-done outcome={}\n",
                match &outcome {
                    Ok(()) => "ok".to_string(),
                    Err(reason) => reason.clone(),
                }
            );
            outcome
        }
    };
    outcome
}

fn main() {
    // Installed outside catch_unwind: they must survive a panic. The console
    // fd itself is opened by real_main once devtmpfs provides /dev/console.
    install_signal_handlers();

    let result = std::panic::catch_unwind(real_main);
    let marker = match result {
        Ok(Ok(())) => "QEMU-TEST: PASS\n".to_string(),
        Ok(Err(reason)) => format!("QEMU-TEST: FAIL({})\n", reason),
        Err(_) => "QEMU-TEST: FAIL(panic)\n".to_string(),
    };
    raw_write(console_fd(), marker.as_bytes());
    say!("QEMU-INIT: poweroff\n");
    power_off();
}
