//! hello-wasi-gui: minimal Wayland SHM + xdg hello for Wawona Runtime (wasm32-wasip1).
//!
//! Paints "Hello from WASI" once, handles configure/resize and close. No seat UI.
//! Build: `./build.sh` (rustup only, no Nix).

mod font;

#[link(wasm_import_module = "env")]
extern "C" {
    fn wawona_wayland_connect(fd_out: *mut i32) -> i32;
    fn wawona_wayland_shm_create(size: i32, fd_out: *mut i32) -> i32;
    fn wawona_wayland_shm_write(shm_fd: i32, offset: i32, buf: *const u8, len: i32) -> i32;
    fn wawona_wayland_sendmsg(wl_fd: i32, buf: *const u8, len: i32, scm_fd: i32) -> i32;
    fn wawona_socket_recv(fd: i32, buf: *mut u8, len: i32, n_out: *mut i32) -> i32;
}

const DISPLAY: u32 = 1;
const FORMAT_XRGB8888: u32 = 1;
const BG: u32 = 0xFF1A_5F7A;
const FG: u32 = 0xFFFF_FFFF;
const DIM: u32 = 0xFFB8_D4E0;
const DEFAULT_W: i32 = 320;
const DEFAULT_H: i32 = 180;
const MIN_W: i32 = 200;
const MIN_H: i32 = 120;
const MAX_W: i32 = 1920;
const MAX_H: i32 = 1080;

struct Conn {
    wl: i32,
    next_id: u32,
    registry: u32,
    compositor: u32,
    shm: u32,
    xdg_wm: u32,
    surface: u32,
    xdg_surface: u32,
    toplevel: u32,
    pool: u32,
    buffer: u32,
    shm_fd: i32,
    width: i32,
    height: i32,
    pending_w: i32,
    pending_h: i32,
    configured: bool,
    need_redraw: bool,
    closed: bool,
}

fn main() {
    let mut wl = 0i32;
    let rc = unsafe { wawona_wayland_connect(&mut wl) };
    if rc != 0 {
        eprintln!("hello-wasi-gui: wayland connect errno={rc} (is WAYLAND_DISPLAY set?)");
        std::process::exit(1);
    }

    let mut c = Conn {
        wl,
        next_id: 2,
        registry: 0,
        compositor: 0,
        shm: 0,
        xdg_wm: 0,
        surface: 0,
        xdg_surface: 0,
        toplevel: 0,
        pool: 0,
        buffer: 0,
        shm_fd: -1,
        width: DEFAULT_W,
        height: DEFAULT_H,
        pending_w: DEFAULT_W,
        pending_h: DEFAULT_H,
        configured: false,
        need_redraw: false,
        closed: false,
    };

    c.registry = c.alloc();
    c.req(DISPLAY, 1, &c.registry.to_le_bytes());
    let cb = c.alloc();
    c.req(DISPLAY, 0, &cb.to_le_bytes());
    c.roundtrip_until_callback(cb);

    if c.compositor == 0 || c.shm == 0 || c.xdg_wm == 0 {
        eprintln!(
            "hello-wasi-gui: missing globals compositor={} shm={} xdg={}",
            c.compositor, c.shm, c.xdg_wm
        );
        std::process::exit(1);
    }

    c.surface = c.alloc();
    c.req(c.compositor, 0, &c.surface.to_le_bytes());
    c.xdg_surface = c.alloc();
    let mut body = Vec::new();
    body.extend_from_slice(&c.xdg_surface.to_le_bytes());
    body.extend_from_slice(&c.surface.to_le_bytes());
    c.req(c.xdg_wm, 2, &body);
    c.toplevel = c.alloc();
    c.req(c.xdg_surface, 1, &c.toplevel.to_le_bytes());
    c.set_title(c.toplevel, "hello-wasi-gui");
    c.set_min_size(c.toplevel, MIN_W, MIN_H);
    c.req(c.surface, 6, &[]);

    while !c.configured && !c.closed {
        if !c.recv_once() {
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    }
    if c.closed {
        std::process::exit(0);
    }

    c.apply_size();
    c.rebuild_buffer_and_paint();
    c.need_redraw = false;
    println!(
        "hello-wasi-gui: {}x{} XRGB8888 committed (close window to exit)",
        c.width, c.height
    );

    while !c.closed {
        if c.recv_once() {
            if c.need_redraw && !c.closed {
                c.apply_size();
                c.rebuild_buffer_and_paint();
                c.need_redraw = false;
            }
        } else if !c.closed {
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    }
    eprintln!("hello-wasi-gui: closed");
}

impl Conn {
    fn alloc(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn req(&self, obj: u32, opcode: u16, body: &[u8]) {
        let mut msg = Vec::new();
        put_header(&mut msg, obj, opcode, 8 + body.len() as u32);
        msg.extend_from_slice(body);
        self.sendmsg(&msg, -1);
    }

    fn sendmsg(&self, msg: &[u8], scm: i32) {
        let rc = unsafe { wawona_wayland_sendmsg(self.wl, msg.as_ptr(), msg.len() as i32, scm) };
        if rc != 0 {
            eprintln!("hello-wasi-gui: sendmsg errno={rc}");
            std::process::exit(1);
        }
    }

    fn set_title(&self, toplevel: u32, title: &str) {
        let mut body = Vec::new();
        put_string(&mut body, title);
        self.req(toplevel, 2, &body);
    }

    fn set_min_size(&self, toplevel: u32, w: i32, h: i32) {
        let mut body = Vec::new();
        body.extend_from_slice(&w.to_le_bytes());
        body.extend_from_slice(&h.to_le_bytes());
        self.req(toplevel, 8, &body);
    }

    fn apply_size(&mut self) {
        let mut w = if self.pending_w > 0 {
            self.pending_w
        } else {
            self.width.max(DEFAULT_W)
        };
        let mut h = if self.pending_h > 0 {
            self.pending_h
        } else {
            self.height.max(DEFAULT_H)
        };
        w = w.clamp(MIN_W, MAX_W);
        h = h.clamp(MIN_H, MAX_H);
        self.width = w;
        self.height = h;
    }

    fn rebuild_buffer_and_paint(&mut self) {
        let stride = self.width * 4;
        let bytes = stride * self.height;
        let mut pixels = vec![0u8; bytes as usize];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&BG.to_le_bytes());
        }
        self.draw_hello(&mut pixels, stride);

        let mut shm_fd = 0i32;
        let rc = unsafe { wawona_wayland_shm_create(bytes, &mut shm_fd) };
        if rc != 0 {
            eprintln!("hello-wasi-gui: shm_create errno={rc}");
            std::process::exit(1);
        }
        let rc = unsafe {
            wawona_wayland_shm_write(shm_fd, 0, pixels.as_ptr(), pixels.len() as i32)
        };
        if rc != 0 {
            eprintln!("hello-wasi-gui: shm_write errno={rc}");
            std::process::exit(1);
        }

        let old_pool = self.pool;
        let old_buffer = self.buffer;

        self.pool = self.alloc();
        let mut pool_msg = Vec::new();
        put_header(&mut pool_msg, self.shm, 0, 16);
        pool_msg.extend_from_slice(&self.pool.to_le_bytes());
        pool_msg.extend_from_slice(&bytes.to_le_bytes());
        self.sendmsg(&pool_msg, shm_fd);
        self.shm_fd = shm_fd;

        self.buffer = self.alloc();
        let mut buf_body = Vec::new();
        buf_body.extend_from_slice(&self.buffer.to_le_bytes());
        buf_body.extend_from_slice(&0i32.to_le_bytes());
        buf_body.extend_from_slice(&self.width.to_le_bytes());
        buf_body.extend_from_slice(&self.height.to_le_bytes());
        buf_body.extend_from_slice(&stride.to_le_bytes());
        buf_body.extend_from_slice(&FORMAT_XRGB8888.to_le_bytes());
        self.req(self.pool, 0, &buf_body);

        let mut attach = Vec::new();
        attach.extend_from_slice(&self.buffer.to_le_bytes());
        attach.extend_from_slice(&0i32.to_le_bytes());
        attach.extend_from_slice(&0i32.to_le_bytes());
        self.req(self.surface, 1, &attach);

        let mut geom = Vec::new();
        geom.extend_from_slice(&0i32.to_le_bytes());
        geom.extend_from_slice(&0i32.to_le_bytes());
        geom.extend_from_slice(&self.width.to_le_bytes());
        geom.extend_from_slice(&self.height.to_le_bytes());
        self.req(self.xdg_surface, 3, &geom);

        let mut damage = Vec::new();
        damage.extend_from_slice(&0i32.to_le_bytes());
        damage.extend_from_slice(&0i32.to_le_bytes());
        damage.extend_from_slice(&self.width.to_le_bytes());
        damage.extend_from_slice(&self.height.to_le_bytes());
        self.req(self.surface, 2, &damage);
        self.req(self.surface, 6, &[]);

        if old_buffer != 0 {
            self.req(old_buffer, 0, &[]);
        }
        if old_pool != 0 {
            self.req(old_pool, 1, &[]);
        }
    }

    fn draw_hello(&self, pixels: &mut [u8], stride: i32) {
        let mut y = self.height / 2 - font::line_height() - 4;
        if y < 12 {
            y = 12;
        }
        font::draw_text(
            pixels,
            stride,
            self.width,
            self.height,
            24,
            y,
            "Hello from WASI",
            FG,
        );
        y += font::line_height() + 8;
        font::draw_text(
            pixels,
            stride,
            self.width,
            self.height,
            24,
            y,
            "hello-wasi-gui · wl_shm",
            DIM,
        );
        y += font::line_height() + 8;
        font::draw_text(
            pixels,
            stride,
            self.width,
            self.height,
            24,
            y,
            &format!("{}x{}", self.width, self.height),
            DIM,
        );
    }

    fn roundtrip_until_callback(&mut self, cb: u32) {
        let mut done = false;
        let mut spins = 0u32;
        while !done {
            let (obj, opcode, payload) = match self.read_event() {
                Some(t) => t,
                None => {
                    spins += 1;
                    if spins > 5000 {
                        eprintln!("hello-wasi-gui: registry sync timed out");
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    continue;
                }
            };
            spins = 0;
            if obj == cb && opcode == 0 {
                done = true;
            } else if obj == self.registry && opcode == 0 {
                self.on_global(&payload);
            }
        }
    }

    fn recv_once(&mut self) -> bool {
        let (obj, opcode, payload) = match self.read_event() {
            Some(t) => t,
            None => return false,
        };
        if obj == self.xdg_wm && opcode == 0 && payload.len() >= 4 {
            self.req(self.xdg_wm, 3, &payload[..4]);
        } else if obj == self.xdg_surface && opcode == 0 && payload.len() >= 4 {
            self.req(self.xdg_surface, 4, &payload[..4]);
            self.configured = true;
            self.need_redraw = true;
        } else if obj == self.toplevel && opcode == 0 && payload.len() >= 8 {
            let w = i32::from_le_bytes(payload[0..4].try_into().unwrap());
            let h = i32::from_le_bytes(payload[4..8].try_into().unwrap());
            if w > 0 {
                self.pending_w = w;
            }
            if h > 0 {
                self.pending_h = h;
            }
        } else if obj == self.toplevel && opcode == 1 {
            self.closed = true;
        }
        true
    }

    fn on_global(&mut self, payload: &[u8]) {
        if payload.len() < 8 {
            return;
        }
        let name = u32::from_le_bytes(payload[0..4].try_into().unwrap());
        let (iface, rest) = take_string(&payload[4..]);
        if rest.len() < 4 {
            return;
        }
        let version = u32::from_le_bytes(rest[0..4].try_into().unwrap());
        let (want, ver) = match iface.as_str() {
            "wl_compositor" => {
                self.compositor = self.alloc();
                (self.compositor, version.min(4))
            }
            "wl_shm" => {
                self.shm = self.alloc();
                (self.shm, 1)
            }
            "xdg_wm_base" => {
                self.xdg_wm = self.alloc();
                (self.xdg_wm, version.min(2))
            }
            _ => return,
        };
        let mut body = Vec::new();
        body.extend_from_slice(&name.to_le_bytes());
        put_string(&mut body, &iface);
        body.extend_from_slice(&ver.to_le_bytes());
        body.extend_from_slice(&want.to_le_bytes());
        self.req(self.registry, 0, &body);
    }

    fn read_event(&self) -> Option<(u32, u16, Vec<u8>)> {
        let mut hdr = [0u8; 8];
        if !recv_exact(self.wl, &mut hdr) {
            return None;
        }
        let obj = u32::from_le_bytes(hdr[0..4].try_into().unwrap());
        let size_op = u32::from_le_bytes(hdr[4..8].try_into().unwrap());
        let size = (size_op >> 16) as usize;
        let opcode = (size_op & 0xffff) as u16;
        if size < 8 {
            return None;
        }
        let mut payload = vec![0u8; size - 8];
        if !payload.is_empty() && !recv_exact(self.wl, &mut payload) {
            return None;
        }
        Some((obj, opcode, payload))
    }
}

fn put_header(out: &mut Vec<u8>, obj: u32, opcode: u16, size: u32) {
    out.extend_from_slice(&obj.to_le_bytes());
    let size_op = (size << 16) | u32::from(opcode);
    out.extend_from_slice(&size_op.to_le_bytes());
}

fn put_string(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let n = bytes.len() + 1;
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(bytes);
    out.push(0);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn take_string(data: &[u8]) -> (String, &[u8]) {
    if data.len() < 4 {
        return (String::new(), data);
    }
    let n = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let padded = n.div_ceil(4) * 4;
    if data.len() < 4 + padded {
        return (String::new(), data);
    }
    let raw = &data[4..4 + n.saturating_sub(1)];
    let s = String::from_utf8_lossy(raw).into_owned();
    (s, &data[4 + padded..])
}

fn recv_exact(fd: i32, dest: &mut [u8]) -> bool {
    let mut off = 0;
    while off < dest.len() {
        let mut n = 0i32;
        let rc = unsafe {
            wawona_socket_recv(
                fd,
                dest[off..].as_mut_ptr(),
                (dest.len() - off) as i32,
                &mut n,
            )
        };
        if rc != 0 || n <= 0 {
            return false;
        }
        off += n as usize;
    }
    true
}
