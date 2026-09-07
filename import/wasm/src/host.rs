//! Host ABIs: `wawona_socket_*`, `wawona_terminal_*`, Wayland fd-bridge.
//!
//! Sockets use POSIX (store-safe on iOS; no MAP_JIT). TLS uses a blocking
//! rustls-free path: `tls_connect_host` currently returns ENOSYS unless the
//! caller uses a WASI-P2 `wasi:http` / rustls guest. TCP/UDP/unix work.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::sync::{Mutex, OnceLock};

const EBADF: i32 = 8;
const EINVAL: i32 = 28;
const ENOSYS: i32 = 52;
const EIO: i32 = 29;
const EACCES: i32 = 2;

static TERMINAL_RAW: OnceLock<Mutex<i32>> = OnceLock::new();
static SOCKS: OnceLock<Mutex<SockTable>> = OnceLock::new();

enum Sock {
    Tcp(TcpStream),
    TcpListen(#[allow(dead_code)] TcpListener),
    Udp(UdpSocket),
    Unix(std::os::unix::net::UnixStream),
    File(std::fs::File),
}

struct SockTable {
    next: i32,
    map: HashMap<i32, Sock>,
}

impl SockTable {
    fn new() -> Self {
        Self {
            next: 3,
            map: HashMap::new(),
        }
    }

    fn insert(&mut self, s: Sock) -> i32 {
        let fd = self.next;
        self.next += 1;
        self.map.insert(fd, s);
        fd
    }
}

fn socks() -> &'static Mutex<SockTable> {
    SOCKS.get_or_init(|| Mutex::new(SockTable::new()))
}

pub fn terminal_raw_enabled() -> i32 {
    *TERMINAL_RAW
        .get_or_init(|| Mutex::new(0))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn terminal_set_raw(enabled: i32) -> i32 {
    *TERMINAL_RAW
        .get_or_init(|| Mutex::new(0))
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = if enabled != 0 { 1 } else { 0 };
    0
}

fn read_guest_str(mem: &[u8], ptr: u32, len: u32) -> Result<String, i32> {
    let start = ptr as usize;
    let end = start.checked_add(len as usize).ok_or(EINVAL)?;
    let bytes = mem.get(start..end).ok_or(EINVAL)?;
    std::str::from_utf8(bytes).map(|s| s.to_string()).map_err(|_| EINVAL)
}

fn write_i32(mem: &mut [u8], ptr: u32, val: i32) -> i32 {
    let start = ptr as usize;
    if start + 4 > mem.len() {
        return EINVAL;
    }
    mem[start..start + 4].copy_from_slice(&val.to_le_bytes());
    0
}

/// Linker host functions for P1 modules (module `wawona_socket` / `wawona_terminal`
/// plus `env` aliases for Rust defaults).
pub fn add_host_imports(linker: &mut wasmtime::Linker<crate::p1::P1State>) -> anyhow::Result<()> {
    for module in ["wawona_socket", "env"] {
        linker.func_wrap(module, "wawona_socket_socket", |mut caller: wasmtime::Caller<'_, crate::p1::P1State>, domain: i32, ty: i32, fd_out: i32| -> i32 {
            socket_socket(&mut caller, domain, ty, fd_out as u32)
        })?;
        linker.func_wrap(module, "wawona_socket_connect_host", |mut caller: wasmtime::Caller<'_, crate::p1::P1State>, fd: i32, host: i32, host_len: i32, port: i32| -> i32 {
            socket_connect_host(&mut caller, fd, host as u32, host_len as u32, port)
        })?;
        linker.func_wrap(module, "wawona_socket_send", |mut caller: wasmtime::Caller<'_, crate::p1::P1State>, fd: i32, buf: i32, len: i32, sent_out: i32| -> i32 {
            socket_send(&mut caller, fd, buf as u32, len as u32, sent_out as u32)
        })?;
        linker.func_wrap(module, "wawona_socket_recv", |mut caller: wasmtime::Caller<'_, crate::p1::P1State>, fd: i32, buf: i32, len: i32, recv_out: i32| -> i32 {
            socket_recv(&mut caller, fd, buf as u32, len as u32, recv_out as u32)
        })?;
        linker.func_wrap(module, "wawona_socket_close", |_caller: wasmtime::Caller<'_, crate::p1::P1State>, fd: i32| -> i32 {
            socket_close(fd)
        })?;
        linker.func_wrap(module, "wawona_socket_tls_connect_host", |_c: wasmtime::Caller<'_, crate::p1::P1State>, _fd: i32, _h: i32, _hl: i32, _p: i32| -> i32 {
            ENOSYS
        })?;
        linker.func_wrap(module, "wawona_wayland_connect", |mut caller: wasmtime::Caller<'_, crate::p1::P1State>, fd_out: i32| -> i32 {
            wayland_connect(&mut caller, fd_out as u32)
        })?;
        linker.func_wrap(module, "wawona_wayland_shm_create", |mut caller: wasmtime::Caller<'_, crate::p1::P1State>, size: i32, fd_out: i32| -> i32 {
            wayland_shm_create(&mut caller, size, fd_out as u32)
        })?;
        linker.func_wrap(module, "wawona_wayland_shm_send", |_c: wasmtime::Caller<'_, crate::p1::P1State>, wl_fd: i32, shm_fd: i32| -> i32 {
            wayland_shm_send(wl_fd, shm_fd)
        })?;
        linker.func_wrap(module, "wawona_wayland_shm_write", |mut caller: wasmtime::Caller<'_, crate::p1::P1State>, shm_fd: i32, offset: i32, buf: i32, len: i32| -> i32 {
            wayland_shm_write(&mut caller, shm_fd, offset, buf as u32, len as u32)
        })?;
        linker.func_wrap(module, "wawona_wayland_sendmsg", |mut caller: wasmtime::Caller<'_, crate::p1::P1State>, wl_fd: i32, buf: i32, len: i32, scm_fd: i32| -> i32 {
            wayland_sendmsg(&mut caller, wl_fd, buf as u32, len as u32, scm_fd)
        })?;
    }
    for module in ["wawona_terminal", "env"] {
        linker.func_wrap(module, "wawona_terminal_set_raw", |_c: wasmtime::Caller<'_, crate::p1::P1State>, enabled: i32| -> i32 {
            terminal_set_raw(enabled)
        })?;
        linker.func_wrap(module, "wawona_terminal_is_tty", |_c: wasmtime::Caller<'_, crate::p1::P1State>, fd: i32| -> i32 {
            if fd >= 0 && fd <= 2 { 1 } else { 0 }
        })?;
    }
    Ok(())
}

fn memory_mut<'a>(caller: &'a mut wasmtime::Caller<'_, crate::p1::P1State>) -> Result<&'a mut [u8], i32> {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or(EIO)?;
    Ok(mem.data_mut(caller))
}

fn socket_socket(caller: &mut wasmtime::Caller<'_, crate::p1::P1State>, domain: i32, ty: i32, fd_out: u32) -> i32 {
    // AF_INET=2, AF_INET6=30, AF_UNIX=1; SOCK_STREAM=1, SOCK_DGRAM=2
    let mut table = socks().lock().unwrap_or_else(|e| e.into_inner());
    let sock = match (domain, ty) {
        (2, 1) | (30, 1) => {
            // Placeholder TCP stream slot — connect_host replaces it.
            match TcpStream::connect("127.0.0.1:9") {
                Ok(s) => Sock::Tcp(s),
                Err(_) => {
                    // Bind a discarded local socket so we have an fd object; connect_host
                    // will replace the table entry.
                    match std::net::TcpListener::bind("127.0.0.1:0")
                        .and_then(|l| l.set_nonblocking(true).map(|_| l))
                    {
                        Ok(l) => Sock::TcpListen(l),
                        Err(_) => return EIO,
                    }
                }
            }
        }
        (2, 2) | (30, 2) => match UdpSocket::bind("0.0.0.0:0") {
            Ok(u) => Sock::Udp(u),
            Err(_) => return EIO,
        },
        (1, 1) => {
            // Unix stream created at connect time; store a dummy file for now.
            match std::fs::File::open("/dev/null") {
                Ok(f) => Sock::File(f),
                Err(_) => return EIO,
            }
        }
        _ => return EINVAL,
    };
    let fd = table.insert(sock);
    drop(table);
    match memory_mut(caller) {
        Ok(mem) => write_i32(mem, fd_out, fd),
        Err(e) => e,
    }
}

fn socket_connect_host(caller: &mut wasmtime::Caller<'_, crate::p1::P1State>, fd: i32, host: u32, host_len: u32, port: i32) -> i32 {
    let mem = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return EIO,
    };
    let data = mem.data(&mut *caller);
    let hostname = match read_guest_str(data, host, host_len) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let addr = format!("{hostname}:{port}");
    match TcpStream::connect(&addr) {
        Ok(stream) => {
            let _ = stream.set_nodelay(true);
            let mut table = socks().lock().unwrap_or_else(|e| e.into_inner());
            table.map.insert(fd, Sock::Tcp(stream));
            0
        }
        Err(_) => EIO,
    }
}

fn socket_send(caller: &mut wasmtime::Caller<'_, crate::p1::P1State>, fd: i32, buf: u32, len: u32, sent_out: u32) -> i32 {
    let mem = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return EIO,
    };
    let data = mem.data(&mut *caller);
    let start = buf as usize;
    let end = match start.checked_add(len as usize) {
        Some(e) => e,
        None => return EINVAL,
    };
    let bytes = match data.get(start..end) {
        Some(b) => b.to_vec(),
        None => return EINVAL,
    };
    let mut table = socks().lock().unwrap_or_else(|e| e.into_inner());
    let n = match table.map.get_mut(&fd) {
        Some(Sock::Tcp(s)) => s.write(&bytes).unwrap_or(0),
        Some(Sock::Unix(s)) => s.write(&bytes).unwrap_or(0),
        Some(Sock::Udp(s)) => s.send(&bytes).unwrap_or(0),
        _ => return EBADF,
    };
    drop(table);
    let data_mut = mem.data_mut(caller);
    write_i32(data_mut, sent_out, n as i32)
}

fn socket_recv(caller: &mut wasmtime::Caller<'_, crate::p1::P1State>, fd: i32, buf: u32, len: u32, recv_out: u32) -> i32 {
    let mut tmp = vec![0u8; len as usize];
    let n = {
        let mut table = socks().lock().unwrap_or_else(|e| e.into_inner());
        match table.map.get_mut(&fd) {
            Some(Sock::Tcp(s)) => s.read(&mut tmp).unwrap_or(0),
            Some(Sock::Unix(s)) => s.read(&mut tmp).unwrap_or(0),
            Some(Sock::Udp(s)) => s.recv(&mut tmp).unwrap_or(0),
            _ => return EBADF,
        }
    };
    let mem = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return EIO,
    };
    let data = mem.data_mut(caller);
    let start = buf as usize;
    if start + n > data.len() {
        return EINVAL;
    }
    data[start..start + n].copy_from_slice(&tmp[..n]);
    write_i32(data, recv_out, n as i32)
}

fn socket_close(fd: i32) -> i32 {
    let mut table = socks().lock().unwrap_or_else(|e| e.into_inner());
    match table.map.remove(&fd) {
        Some(_) => 0,
        None => EBADF,
    }
}

fn default_xdg_runtime_dir() -> String {
    // Match Wawona compositor-host / macos-register-cli-bins (`/tmp/wawona-$UID`).
    // Falling back to bare `/tmp` made host `wasm` miss the live socket when
    // install never exported XDG_RUNTIME_DIR into the interactive shell.
    let uid = unsafe { libc::getuid() };
    format!("/tmp/wawona-{uid}")
}

fn wayland_connect(caller: &mut wasmtime::Caller<'_, crate::p1::P1State>, fd_out: u32) -> i32 {
    let xdg = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| default_xdg_runtime_dir());
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    let path = format!("{xdg}/{display}");
    match std::os::unix::net::UnixStream::connect(&path) {
        Ok(s) => {
            // Leave blocking by default. The guest paints after each event so a
            // blocking recv still handles configure / input. Nonblocking can be
            // enabled later once SCM_RIGHTS + poll are proven together.
            let mut table = socks().lock().unwrap_or_else(|e| e.into_inner());
            let fd = table.insert(Sock::Unix(s));
            drop(table);
            match memory_mut(caller) {
                Ok(mem) => write_i32(mem, fd_out, fd),
                Err(e) => e,
            }
        }
        Err(_) => EIO,
    }
}

fn wayland_shm_create(caller: &mut wasmtime::Caller<'_, crate::p1::P1State>, size: i32, fd_out: u32) -> i32 {
    if size <= 0 {
        return EINVAL;
    }
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| default_xdg_runtime_dir());
    let path = format!("{dir}/wwn-wasm-shm-{}-{}", std::process::id(), size);
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(_) => return EIO,
    };
    if file.set_len(size as u64).is_err() {
        return EIO;
    }
    let _ = std::fs::remove_file(&path);
    let mut table = socks().lock().unwrap_or_else(|e| e.into_inner());
    let fd = table.insert(Sock::File(file));
    drop(table);
    match memory_mut(caller) {
        Ok(mem) => write_i32(mem, fd_out, fd),
        Err(e) => e,
    }
}

fn wayland_shm_write(
    caller: &mut wasmtime::Caller<'_, crate::p1::P1State>,
    shm_fd: i32,
    offset: i32,
    buf: u32,
    len: u32,
) -> i32 {
    if offset < 0 {
        return EINVAL;
    }
    let mem = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return EIO,
    };
    let data = mem.data(&mut *caller);
    let start = buf as usize;
    let end = match start.checked_add(len as usize) {
        Some(e) => e,
        None => return EINVAL,
    };
    let bytes = match data.get(start..end) {
        Some(b) => b.to_vec(),
        None => return EINVAL,
    };
    let mut table = socks().lock().unwrap_or_else(|e| e.into_inner());
    match table.map.get_mut(&shm_fd) {
        Some(Sock::File(f)) => {
            use std::io::{Seek, SeekFrom};
            if f.seek(SeekFrom::Start(offset as u64)).is_err() {
                return EIO;
            }
            if f.write_all(&bytes).is_err() {
                return EIO;
            }
            0
        }
        _ => EBADF,
    }
}

fn wayland_sendmsg(
    caller: &mut wasmtime::Caller<'_, crate::p1::P1State>,
    wl_fd: i32,
    buf: u32,
    len: u32,
    scm_fd: i32,
) -> i32 {
    let mem = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return EIO,
    };
    let data = mem.data(&mut *caller);
    let start = buf as usize;
    let end = match start.checked_add(len as usize) {
        Some(e) => e,
        None => return EINVAL,
    };
    let mut bytes = match data.get(start..end) {
        Some(b) => b.to_vec(),
        None => return EINVAL,
    };
    let table = socks().lock().unwrap_or_else(|e| e.into_inner());
    let wl_raw = match table.map.get(&wl_fd) {
        Some(Sock::Unix(s)) => s.as_raw_fd(),
        _ => return EBADF,
    };
    let scm_raw = if scm_fd >= 0 {
        match table.map.get(&scm_fd) {
            Some(Sock::File(f)) => Some(f.as_raw_fd()),
            _ => return EBADF,
        }
    } else {
        None
    };
    drop(table);
    sendmsg_bytes(wl_raw, &mut bytes, scm_raw)
}

fn sendmsg_bytes(wl_raw: RawFd, bytes: &mut [u8], scm_raw: Option<RawFd>) -> i32 {
    unsafe {
        let mut iov = libc::iovec {
            iov_base: bytes.as_mut_ptr() as *mut _,
            iov_len: bytes.len(),
        };
        let mut cmsg_buf = [0u8; 64];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        if let Some(shm_raw) = scm_raw {
            msg.msg_control = cmsg_buf.as_mut_ptr() as *mut _;
            msg.msg_controllen = cmsg_buf.len() as _;
            let cmsg = libc::CMSG_FIRSTHDR(&msg);
            if cmsg.is_null() {
                return EIO;
            }
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as _;
            let data = libc::CMSG_DATA(cmsg) as *mut RawFd;
            *data = shm_raw;
            msg.msg_controllen = (*cmsg).cmsg_len;
        }
        if libc::sendmsg(wl_raw, &msg, 0) < 0 {
            return EACCES;
        }
    }
    0
}

fn wayland_shm_send(wl_fd: i32, shm_fd: i32) -> i32 {
    let table = socks().lock().unwrap_or_else(|e| e.into_inner());
    let wl_raw = match table.map.get(&wl_fd) {
        Some(Sock::Unix(s)) => s.as_raw_fd(),
        _ => return EBADF,
    };
    let shm_raw = match table.map.get(&shm_fd) {
        Some(Sock::File(f)) => f.as_raw_fd(),
        _ => return EBADF,
    };
    drop(table);
    // sendmsg SCM_RIGHTS: one dummy byte + the shm fd.
    unsafe {
        let mut dummy = [0u8; 1];
        let mut iov = libc::iovec {
            iov_base: dummy.as_mut_ptr() as *mut _,
            iov_len: 1,
        };
        let mut cmsg_buf = [0u8; 64];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cmsg_buf.as_mut_ptr() as *mut _;
        msg.msg_controllen = cmsg_buf.len() as _;
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        if cmsg.is_null() {
            return EIO;
        }
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as _;
        let data = libc::CMSG_DATA(cmsg) as *mut RawFd;
        *data = shm_raw;
        msg.msg_controllen = (*cmsg).cmsg_len;
        if libc::sendmsg(wl_raw, &msg, 0) < 0 {
            return EACCES;
        }
    }
    0
}

/// Used by tests / C `wawona_terminal_raw_enabled`.
#[allow(dead_code)]
pub fn reset_terminal() {
    terminal_set_raw(0);
}

// Keep FromRawFd/IntoRawFd imported for future host-fd adoption.
#[allow(dead_code)]
fn _fd_markers() {
    let _ = FromRawFd::from_raw_fd as unsafe fn(RawFd) -> std::fs::File;
    let _ = IntoRawFd::into_raw_fd as fn(std::fs::File) -> RawFd;
}
