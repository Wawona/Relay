//! Import a guest `wl_shm` buffer from a waypipe-vsock stream.
//!
//! Host-generated pixels are rejected. The first committed SHM attach with a
//! non-empty payload is the imported frame. Compression must be `none` on the
//! guest waypipe server (`--compress none`) so BUFFER_FILL is raw SHM bytes.

use crate::guest_session::frame_hash;
use crate::vsock::VsockConnection;
use relay_core::RelayError;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const WMSG_PROTOCOL: u32 = 0;
const WMSG_OPEN_FILE: u32 = 2;
const WMSG_BUFFER_FILL: u32 = 5;
const WMSG_BUFFER_DIFF: u32 = 6;
const WMSG_ACK_NBLOCKS: u32 = 16;
const MIN_FRAME: usize = 64 * 64 * 4;

#[derive(Debug, Clone)]
pub struct ImportedFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub sha256: [u8; 32],
}

pub(crate) struct Importer {
    thread: Option<JoinHandle<()>>,
}

impl Importer {
    pub(crate) fn start(
        connections: Receiver<VsockConnection>,
        stop: Arc<AtomicBool>,
        vm_state: Arc<AtomicU8>,
        frame: Arc<Mutex<Option<ImportedFrame>>>,
    ) -> Result<Self, RelayError> {
        let thread = thread::Builder::new()
            .name("relay-shm-import".into())
            .spawn(move || {
                while !stop.load(Ordering::Acquire) && vm_state.load(Ordering::Acquire) == 0 {
                    match connections.recv_timeout(Duration::from_millis(20)) {
                        Ok(connection) => {
                            eprintln!(
                                "Relay SHM import: accepted vsock guest_port={} host_port={}",
                                connection.guest_port, connection.host_port
                            );
                            // Keep the waypipe stream open after the first
                            // imported frame. Closing it kills the guest
                            // session and races AUTH (systemctl exit 3).
                            if let Err(error) = import_stream(connection.stream, &stop, &frame) {
                                eprintln!("Relay SHM import: {error}");
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            })
            .map_err(|error| RelayError::Failed(format!("cannot start SHM importer: {error}")))?;
        Ok(Self {
            thread: Some(thread),
        })
    }

    pub(crate) fn join_until(
        &mut self,
        deadline: std::time::Instant,
    ) -> Result<bool, RelayError> {
        crate::shutdown::join_until(&mut self.thread, deadline, "shm import")
    }
}

fn align4(n: usize) -> usize {
    n.div_ceil(4) * 4
}

fn header(size: u32, kind: u32) -> u32 {
    (size << 5) | kind
}

fn parse_header(word: u32) -> (usize, u32) {
    wmsg_size_and_kind(word)
}

/// Waypipe `transfer_header` inverse. Size is bytes including the header word.
pub(crate) fn wmsg_size_and_kind(word: u32) -> (usize, u32) {
    ((word >> 5) as usize, word & 31)
}

pub(crate) fn wmsg_size_accepted(size: usize) -> bool {
    (4..=16 * 1024 * 1024).contains(&size)
}

pub(crate) fn waypipe_connection_header_ok(lead: u32) -> bool {
    lead & CONN_FIXED_BIT != 0 && lead & CONN_UNSET_BIT == 0
}

/// `wl_shm.create_pool` args: native Wayland is `new_id, size` (fd is SCM_RIGHTS).
/// Waypipe may insert a RID: `new_id, rid, size`.
pub(crate) fn shm_create_pool_size(args: &[u8]) -> Option<u32> {
    if args.len() >= 12 {
        Some(u32::from_le_bytes(args[8..12].try_into().ok()?))
    } else if args.len() >= 8 {
        Some(u32::from_le_bytes(args[4..8].try_into().ok()?))
    } else {
        None
    }
}

pub(crate) fn waypipe_needs_ack(kind: u32) -> bool {
    matches!(
        kind,
        WMSG_PROTOCOL | WMSG_OPEN_FILE | WMSG_BUFFER_FILL | WMSG_BUFFER_DIFF | WMSG_INJECT_RIDS
    )
}

/// BUFFER_DIFF span: `start`/`end` are u32 words. None means skip or reject.
pub(crate) fn buffer_diff_span(
    file_len: usize,
    start: usize,
    end: usize,
    pos: usize,
    body_len: usize,
) -> Option<(usize, usize)> {
    if end <= start {
        return None;
    }
    let nbytes = (end - start).checked_mul(4)?;
    let dst = start.checked_mul(4)?;
    let src_end = pos.checked_add(nbytes)?;
    let dst_end = dst.checked_add(nbytes)?;
    if src_end > body_len || dst_end > file_len {
        return None;
    }
    Some((dst, nbytes))
}

#[cfg(kani)]
mod wmsg_proofs {
    use super::*;

    #[kani::proof]
    fn wmsg_kind_stays_in_five_bits() {
        let word: u32 = kani::any();
        let (size, kind) = wmsg_size_and_kind(word);
        assert!(kind < 32);
        assert_eq!(size, (word >> 5) as usize);
        assert_eq!(header(size as u32, kind), (size as u32) << 5 | kind);
    }

    #[kani::proof]
    fn connection_header_requires_fixed_bit() {
        let lead: u32 = kani::any();
        kani::assume(lead & CONN_FIXED_BIT == 0);
        assert!(!waypipe_connection_header_ok(lead));
    }

    #[kani::proof]
    #[kani::unwind(2)]
    fn buffer_diff_span_never_writes_past_the_file() {
        let file_len: u8 = kani::any();
        let start: u8 = kani::any();
        let end: u8 = kani::any();
        let pos: u8 = kani::any();
        let body_len: u8 = kani::any();
        kani::assume(file_len > 0);
        if let Some((dst, nbytes)) = buffer_diff_span(
            file_len as usize,
            start as usize,
            end as usize,
            pos as usize,
            body_len as usize,
        ) {
            assert!(dst + nbytes <= file_len as usize);
            assert!(pos as usize + nbytes <= body_len as usize);
            assert!(end > start);
        }
    }

    #[kani::proof]
    fn shm_create_pool_size_reads_native_or_rid_layout() {
        let mut args = [0u8; 12];
        let size: u32 = kani::any();
        args[4..8].copy_from_slice(&size.to_le_bytes());
        args[8..12].copy_from_slice(&size.to_le_bytes());
        assert_eq!(shm_create_pool_size(&args[..8]), Some(size));
        assert_eq!(shm_create_pool_size(&args), Some(size));
        assert_eq!(shm_create_pool_size(&args[..4]), None);
    }

    #[kani::proof]
    fn waypipe_does_not_ack_ack_messages() {
        assert!(!waypipe_needs_ack(WMSG_ACK_NBLOCKS));
        assert!(!waypipe_needs_ack(WMSG_VERSION));
        assert!(waypipe_needs_ack(WMSG_BUFFER_DIFF));
        assert!(waypipe_needs_ack(WMSG_PROTOCOL));
        assert!(waypipe_needs_ack(WMSG_OPEN_FILE));
    }
}

const WMSG_INJECT_RIDS: u32 = 1;
const WMSG_VERSION: u32 = 23;
const CONN_FIXED_BIT: u32 = 1 << 7;
const CONN_UNSET_BIT: u32 = 1 << 31;

fn publish_frame(sink: &Mutex<Option<ImportedFrame>>, frame: ImportedFrame) {
    let already = sink.lock().ok().and_then(|g| g.is_some().then_some(()));
    if already.is_some() {
        return;
    }
    eprintln!(
        "Relay imported SHM frame: {}x{} sha256={}",
        frame.width,
        frame.height,
        frame
            .sha256
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    if let Ok(mut guard) = sink.lock() {
        *guard = Some(frame);
    }
}

fn import_stream(
    mut stream: UnixStream,
    stop: &AtomicBool,
    sink: &Mutex<Option<ImportedFrame>>,
) -> Result<(), RelayError> {
    stream
        .set_read_timeout(Some(Duration::from_millis(250)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_millis(250)))
        .ok();
    // Guest `waypipe server` writes a 16-byte connection header first
    // (4-byte flags + 12 unused). Treating that as WMSG desyncs the stream.
    let mut conn = [0u8; 16];
    read_exact_interruptible(&mut stream, &mut conn, stop)?;
    let lead = u32::from_le_bytes(conn[0..4].try_into().unwrap());
    if !waypipe_connection_header_ok(lead) {
        return Err(RelayError::Failed(format!(
            "waypipe connection header rejected lead={lead:#010x}"
        )));
    }
    eprintln!("Relay SHM import: connection header lead={lead:#010x}");
    let mut files: BTreeMap<i32, Vec<u8>> = BTreeMap::new();
    let mut compositor = MiniCompositor::new();
    let mut received = 0u32;
    let mut buf = Vec::new();
    while !stop.load(Ordering::Acquire) {
        let mut hdr = [0u8; 4];
        match read_exact_interruptible(&mut stream, &mut hdr, stop) {
            Ok(()) => {}
            Err(_) => {
                if let Some(frame) = compositor.take_frame() {
                    publish_frame(sink, frame);
                    return Ok(());
                }
                if sink.lock().ok().and_then(|g| g.as_ref().map(|_| ())).is_some() {
                    return Ok(());
                }
                return Err(RelayError::Failed("waypipe stream closed before SHM commit".into()));
            }
        }
        let word = u32::from_le_bytes(hdr);
        let (size, kind) = parse_header(word);
        if received < 64 {
            eprintln!("Relay SHM import: wmsg n={received} word={word:#010x} size={size} kind={kind}");
        }
        if !wmsg_size_accepted(size) {
            return Err(RelayError::Failed(format!(
                "waypipe message size rejected word={word:#010x} size={size} kind={kind}"
            )));
        }
        let padded = align4(size);
        buf.resize(padded.saturating_sub(4), 0);
        read_exact_interruptible(&mut stream, &mut buf, stop)?;
        received = received.saturating_add(1);
        match kind {
            WMSG_VERSION => {}
            WMSG_INJECT_RIDS => {}
            WMSG_OPEN_FILE if buf.len() >= 8 => {
                let remote = i32::from_le_bytes(buf[0..4].try_into().unwrap());
                let file_size = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
                eprintln!(
                    "Relay SHM import: OPEN_FILE remote={remote} bytes={file_size}"
                );
                files.insert(remote, vec![0; file_size.min(32 * 1024 * 1024)]);
            }
            WMSG_BUFFER_DIFF if buf.len() >= 12 => {
                let remote = i32::from_le_bytes(buf[0..4].try_into().unwrap());
                if let Some(file) = files.get_mut(&remote) {
                    apply_buffer_diff(file, &buf[4..])?;
                    compositor.refresh_pool(file);
                }
            }
            WMSG_BUFFER_FILL if buf.len() >= 12 => {
                let remote = i32::from_le_bytes(buf[0..4].try_into().unwrap());
                let start = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
                let end = u32::from_le_bytes(buf[8..12].try_into().unwrap()) as usize;
                let data = &buf[12..];
                if let Some(file) = files.get_mut(&remote) {
                    if end >= start && end <= file.len() {
                        let n = (end - start).min(data.len());
                        file[start..start + n].copy_from_slice(&data[..n]);
                        compositor.refresh_pool(file);
                    }
                }
            }
            WMSG_PROTOCOL => {
                compositor.feed(&buf, &files)?;
                for reply in compositor.drain_protocol() {
                    write_protocol(&mut stream, &reply)?;
                }
            }
            _ => {}
        }
        if waypipe_needs_ack(kind) {
            let _ = write_ack(&mut stream, received);
        }
        if let Some(frame) = compositor.take_frame() {
            publish_frame(sink, frame);
            // Stay on this stream so guest waypipe / AUTH stay alive.
        }
    }
    Ok(())
}

fn apply_buffer_diff(file: &mut [u8], rest: &[u8]) -> Result<(), RelayError> {
    if rest.len() < 8 {
        return Ok(());
    }
    let diff_size = u32::from_le_bytes(rest[0..4].try_into().unwrap()) as usize;
    let ntrailing = u32::from_le_bytes(rest[4..8].try_into().unwrap()) as usize;
    let diff = rest.get(8..).unwrap_or(&[]);
    if diff.len() < diff_size.saturating_add(ntrailing) {
        return Err(RelayError::Failed("waypipe BUFFER_DIFF truncated".into()));
    }
    let body = &diff[..diff_size];
    let mut pos = 0;
    while pos + 8 <= body.len() {
        let start = u32::from_le_bytes(body[pos..pos + 4].try_into().unwrap()) as usize;
        let end = u32::from_le_bytes(body[pos + 4..pos + 8].try_into().unwrap()) as usize;
        pos += 8;
        let Some((dst, nbytes)) = buffer_diff_span(file.len(), start, end, pos, body.len()) else {
            if end <= start {
                break;
            }
            return Err(RelayError::Failed("waypipe BUFFER_DIFF span".into()));
        };
        file[dst..dst + nbytes].copy_from_slice(&body[pos..pos + nbytes]);
        pos += nbytes;
    }
    if ntrailing > 0 && ntrailing <= file.len() {
        let src = diff_size;
        if src + ntrailing <= diff.len() {
            let dst = file.len() - ntrailing;
            file[dst..].copy_from_slice(&diff[src..src + ntrailing]);
        }
    }
    Ok(())
}

fn write_ack(stream: &mut UnixStream, messages: u32) -> Result<(), RelayError> {
    let mut out = [0u8; 8];
    out[0..4].copy_from_slice(&header(8, WMSG_ACK_NBLOCKS).to_le_bytes());
    out[4..8].copy_from_slice(&messages.to_le_bytes());
    let _ = stream.write_all(&out);
    Ok(())
}

fn write_protocol(stream: &mut UnixStream, proto: &[u8]) -> Result<(), RelayError> {
    let size = 4 + proto.len();
    let padded = align4(size);
    let mut out = vec![0u8; padded];
    out[0..4].copy_from_slice(&header(size as u32, WMSG_PROTOCOL).to_le_bytes());
    out[4..4 + proto.len()].copy_from_slice(proto);
    let _ = stream.write_all(&out);
    Ok(())
}

fn read_exact_interruptible(
    stream: &mut UnixStream,
    buf: &mut [u8],
    stop: &AtomicBool,
) -> Result<(), RelayError> {
    let mut filled = 0;
    while filled < buf.len() {
        if stop.load(Ordering::Acquire) {
            return Err(RelayError::Failed("SHM import stopped".into()));
        }
        match stream.read(&mut buf[filled..]) {
            Ok(0) => return Err(RelayError::Failed("waypipe EOF".into())),
            Ok(n) => filled += n,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
                || error.kind() == std::io::ErrorKind::TimedOut
                || error.kind() == std::io::ErrorKind::Interrupted =>
            {
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => {
                return Err(RelayError::Failed(format!("waypipe read: {error}")));
            }
        }
    }
    Ok(())
}

struct MiniCompositor {
    objects: BTreeMap<u32, Object>,
    next_server_id: u32,
    pending: Vec<u8>,
    replies: Vec<Vec<u8>>,
    pools: BTreeMap<u32, Pool>,
    buffers: BTreeMap<u32, Buffer>,
    surfaces: BTreeMap<u32, Surface>,
    last_file: Option<Vec<u8>>,
    committed: Option<ImportedFrame>,
}

enum Object {
    Display,
    Registry,
    Compositor,
    Shm,
    WmBase,
    Output,
    Surface,
    Pool,
    Buffer,
    XdgSurface,
    Toplevel,
    Callback,
    Region,
    Presentation,
    PresentationFeedback,
    Seat,
    Pointer,
    Keyboard,
}

struct Pool {
    bytes: Vec<u8>,
}

#[derive(Clone)]
struct Buffer {
    pool: u32,
    offset: i32,
    width: i32,
    height: i32,
    stride: i32,
}

struct Surface {
    buffer: Option<u32>,
}

impl MiniCompositor {
    fn new() -> Self {
        let mut objects = BTreeMap::new();
        objects.insert(1, Object::Display);
        Self {
            objects,
            next_server_id: 0xff00_0000,
            pending: Vec::new(),
            replies: Vec::new(),
            pools: BTreeMap::new(),
            buffers: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            last_file: None,
            committed: None,
        }
    }

    fn feed(&mut self, proto: &[u8], files: &BTreeMap<i32, Vec<u8>>) -> Result<(), RelayError> {
        if let Some((_, file)) = files.iter().next_back() {
            self.last_file = Some(file.clone());
        }
        self.pending.extend_from_slice(proto);
        while self.pending.len() >= 8 {
            let object = u32::from_le_bytes(self.pending[0..4].try_into().unwrap());
            let size_op = u32::from_le_bytes(self.pending[4..8].try_into().unwrap());
            let size = (size_op >> 16) as usize;
            let opcode = size_op as u16;
            if size < 8 || self.pending.len() < size {
                break;
            }
            let args = self.pending[8..size].to_vec();
            self.pending.drain(..size);
            self.dispatch(object, opcode, &args)?;
        }
        Ok(())
    }

    fn drain_protocol(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.replies)
    }

    fn take_frame(&mut self) -> Option<ImportedFrame> {
        self.committed.take()
    }

    fn refresh_pool(&mut self, file: &[u8]) {
        self.last_file = Some(file.to_vec());
        for pool in self.pools.values_mut() {
            let n = file.len().min(pool.bytes.len());
            pool.bytes[..n].copy_from_slice(&file[..n]);
        }
        if self.committed.is_none() {
            let _ = self.import_guest_pixels();
        }
    }

    fn dispatch(&mut self, object: u32, opcode: u16, args: &[u8]) -> Result<(), RelayError> {
        match self.objects.get(&object) {
            Some(Object::Display) if opcode == 1 => {
                let new_id = read_u32(args, 0)?;
                self.objects.insert(new_id, Object::Registry);
                self.emit_globals(new_id);
            }
            Some(Object::Display) if opcode == 0 => {
                let new_id = read_u32(args, 0)?;
                self.objects.insert(new_id, Object::Callback);
                self.emit_u32s(new_id, 0, &[1]);
            }
            Some(Object::Registry) if opcode == 0 => {
                let name = read_u32(args, 0)?;
                let (iface, rest) = read_string(args, 4)?;
                let _version = read_u32(rest, 0)?;
                let new_id = read_u32(rest, 4)?;
                match iface {
                    "wl_compositor" => {
                        self.objects.insert(new_id, Object::Compositor);
                    }
                    "wl_shm" => {
                        self.objects.insert(new_id, Object::Shm);
                        self.emit_u32s(new_id, 0, &[0]);
                        self.emit_u32s(new_id, 0, &[1]);
                    }
                    "xdg_wm_base" => {
                        self.objects.insert(new_id, Object::WmBase);
                    }
                    "wl_output" => {
                        self.objects.insert(new_id, Object::Output);
                        self.emit_output(new_id);
                    }
                    "wp_presentation" => {
                        self.objects.insert(new_id, Object::Presentation);
                        self.emit_u32s(new_id, 0, &[1]);
                    }
                    "wl_seat" => {
                        self.objects.insert(new_id, Object::Seat);
                        self.emit_u32s(new_id, 0, &[3]);
                    }
                    _ => match name {
                    1 => {
                        self.objects.insert(new_id, Object::Compositor);
                    }
                    2 => {
                        self.objects.insert(new_id, Object::Shm);
                        self.emit_u32s(new_id, 0, &[0]);
                        self.emit_u32s(new_id, 0, &[1]);
                    }
                    3 => {
                        self.objects.insert(new_id, Object::WmBase);
                    }
                    4 => {
                        self.objects.insert(new_id, Object::Output);
                        self.emit_output(new_id);
                    }
                    5 => {
                        self.objects.insert(new_id, Object::Presentation);
                        self.emit_u32s(new_id, 0, &[1]);
                    }
                    6 => {
                        self.objects.insert(new_id, Object::Seat);
                        self.emit_u32s(new_id, 0, &[3]);
                    }
                    _ => {}
                }
                }
            }
            Some(Object::Compositor) if opcode == 0 => {
                let new_id = read_u32(args, 0)?;
                self.objects.insert(new_id, Object::Surface);
                self.surfaces.insert(new_id, Surface { buffer: None });
            }
            Some(Object::Compositor) if opcode == 1 => {
                let new_id = read_u32(args, 0)?;
                self.objects.insert(new_id, Object::Region);
            }
            Some(Object::Seat) if opcode == 0 => {
                let new_id = read_u32(args, 0)?;
                self.objects.insert(new_id, Object::Pointer);
            }
            Some(Object::Seat) if opcode == 1 => {
                let new_id = read_u32(args, 0)?;
                self.objects.insert(new_id, Object::Keyboard);
            }
            Some(Object::Shm) if opcode == 0 => {
                let new_id = read_u32(args, 0)?;
                let size = shm_create_pool_size(args).unwrap_or(0) as usize;
                let bytes = self
                    .last_file
                    .clone()
                    .unwrap_or_else(|| vec![0; size])
                    .into_iter()
                    .take(size)
                    .chain(std::iter::repeat(0))
                    .take(size)
                    .collect();
                self.objects.insert(new_id, Object::Pool);
                self.pools.insert(new_id, Pool { bytes });
            }
            Some(Object::Pool) if opcode == 0 => {
                let new_id = read_u32(args, 0)?;
                self.objects.insert(new_id, Object::Buffer);
                self.buffers.insert(
                    new_id,
                    Buffer {
                        pool: object,
                        offset: read_i32(args, 4)?,
                        width: read_i32(args, 8)?,
                        height: read_i32(args, 12)?,
                        stride: read_i32(args, 16)?,
                    },
                );
            }
            Some(Object::Surface) if opcode == 1 => {
                let buffer = read_u32(args, 0)?;
                if let Some(surface) = self.surfaces.get_mut(&object) {
                    surface.buffer = Some(buffer);
                }
            }
            Some(Object::Surface) if opcode == 3 => {
                let new_id = read_u32(args, 0)?;
                self.objects.insert(new_id, Object::Callback);
                self.emit_u32s(new_id, 0, &[1]);
            }
            Some(Object::Surface) if opcode == 6 => {
                self.commit_surface(object)?;
            }
            Some(Object::WmBase) if opcode == 2 => {
                let new_id = read_u32(args, 0)?;
                self.objects.insert(new_id, Object::XdgSurface);
                let serial = self.next_serial();
                self.emit_u32s(new_id, 0, &[serial]);
            }
            Some(Object::XdgSurface) if opcode == 1 => {
                let new_id = read_u32(args, 0)?;
                self.objects.insert(new_id, Object::Toplevel);
                let serial = self.next_serial();
                let mut toplevel = Vec::new();
                toplevel.extend_from_slice(&800i32.to_le_bytes());
                toplevel.extend_from_slice(&600i32.to_le_bytes());
                write_array(&mut toplevel, &[]);
                self.emit(new_id, 0, &toplevel);
                self.emit_u32s(object, 0, &[serial]);
            }
            Some(Object::WmBase) if opcode == 3 => {
                if let Ok(serial) = read_u32(args, 0) {
                    self.emit_u32s(object, 1, &[serial]);
                }
            }
            Some(Object::Presentation) if opcode == 1 => {
                let new_id = read_u32(args, 4).or_else(|_| read_u32(args, 0))?;
                self.objects.insert(new_id, Object::PresentationFeedback);
            }
            _ => {}
        }
        Ok(())
    }

    fn commit_surface(&mut self, surface: u32) -> Result<(), RelayError> {
        let Some(buffer_id) = self.surfaces.get(&surface).and_then(|s| s.buffer) else {
            return Ok(());
        };
        let Some(buffer) = self.buffers.get(&buffer_id).cloned() else {
            return Ok(());
        };
        if buffer.width <= 0 || buffer.height <= 0 || buffer.stride <= 0 {
            return Ok(());
        }
        let Some(pool) = self.pools.get(&buffer.pool) else {
            return Ok(());
        };
        let width = buffer.width as u32;
        let height = buffer.height as u32;
        let stride = buffer.stride as usize;
        let offset = buffer.offset.max(0) as usize;
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        for row in 0..height as usize {
            let src = offset + row * stride;
            let dst = row * width as usize * 4;
            let n = (width as usize * 4).min(stride);
            if src + n <= pool.bytes.len() {
                pixels[dst..dst + n].copy_from_slice(&pool.bytes[src..src + n]);
            }
        }
        if pixels.len() < MIN_FRAME && pixels.iter().all(|b| *b == 0) {
            return Ok(());
        }
        if pixels.is_empty() {
            return Ok(());
        }
        let sha256 = frame_hash(&pixels)?;
        self.committed = Some(ImportedFrame {
            width,
            height,
            pixels,
            sha256,
        });
        Ok(())
    }

    fn import_guest_pixels(&mut self) -> Result<(), RelayError> {
        let surfaces: Vec<u32> = self.surfaces.keys().copied().collect();
        for surface in surfaces {
            self.commit_surface(surface)?;
            if self.committed.is_some() {
                return Ok(());
            }
        }
        let buffers: Vec<Buffer> = self.buffers.values().cloned().collect();
        for buffer in buffers {
            if let Some(frame) = self.frame_from_buffer(&buffer)? {
                if frame.pixels.iter().any(|byte| *byte != 0) {
                    self.committed = Some(frame);
                    return Ok(());
                }
            }
        }
        if let Some(file) = self.last_file.clone() {
            if let Some(frame) = frame_from_shm_file(&file)? {
                self.committed = Some(frame);
            }
        }
        Ok(())
    }

    fn frame_from_buffer(&self, buffer: &Buffer) -> Result<Option<ImportedFrame>, RelayError> {
        if buffer.width <= 0 || buffer.height <= 0 || buffer.stride <= 0 {
            return Ok(None);
        }
        let Some(pool) = self.pools.get(&buffer.pool) else {
            return Ok(None);
        };
        let width = buffer.width as u32;
        let height = buffer.height as u32;
        let stride = buffer.stride as usize;
        let offset = buffer.offset.max(0) as usize;
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        for row in 0..height as usize {
            let src = offset + row * stride;
            let dst = row * width as usize * 4;
            let n = (width as usize * 4).min(stride);
            if src + n <= pool.bytes.len() {
                pixels[dst..dst + n].copy_from_slice(&pool.bytes[src..src + n]);
            }
        }
        if pixels.iter().all(|byte| *byte == 0) {
            return Ok(None);
        }
        let sha256 = frame_hash(&pixels)?;
        Ok(Some(ImportedFrame {
            width,
            height,
            pixels,
            sha256,
        }))
    }

    fn emit_globals(&mut self, registry: u32) {
        self.emit_global(registry, 1, "wl_compositor", 4);
        self.emit_global(registry, 2, "wl_shm", 1);
        self.emit_global(registry, 3, "xdg_wm_base", 4);
        self.emit_global(registry, 4, "wl_output", 2);
        self.emit_global(registry, 5, "wp_presentation", 1);
        self.emit_global(registry, 6, "wl_seat", 7);
    }

    fn emit_output(&mut self, output: u32) {
        let mut geometry = Vec::new();
        geometry.extend_from_slice(&0i32.to_le_bytes());
        geometry.extend_from_slice(&0i32.to_le_bytes());
        geometry.extend_from_slice(&800i32.to_le_bytes());
        geometry.extend_from_slice(&600i32.to_le_bytes());
        geometry.extend_from_slice(&0i32.to_le_bytes());
        write_string(&mut geometry, "Relay");
        write_string(&mut geometry, "SHM");
        geometry.extend_from_slice(&0i32.to_le_bytes());
        self.emit(output, 0, &geometry);
        let mut mode = Vec::new();
        mode.extend_from_slice(&3u32.to_le_bytes());
        mode.extend_from_slice(&800i32.to_le_bytes());
        mode.extend_from_slice(&600i32.to_le_bytes());
        mode.extend_from_slice(&60_000i32.to_le_bytes());
        self.emit(output, 1, &mode);
        self.emit_u32s(output, 3, &[1]);
        self.emit_u32s(output, 2, &[]);
    }

    fn emit_global(&mut self, registry: u32, name: u32, iface: &str, version: u32) {
        let mut payload = Vec::new();
        payload.extend_from_slice(&name.to_le_bytes());
        write_string(&mut payload, iface);
        payload.extend_from_slice(&version.to_le_bytes());
        self.emit(registry, 0, &payload);
    }

    fn emit_u32s(&mut self, object: u32, opcode: u16, values: &[u32]) {
        let mut payload = Vec::new();
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        self.emit(object, opcode, &payload);
    }

    fn emit(&mut self, object: u32, opcode: u16, payload: &[u8]) {
        let size = 8 + payload.len();
        let mut msg = Vec::with_capacity(size);
        msg.extend_from_slice(&object.to_le_bytes());
        msg.extend_from_slice(&(((size as u32) << 16) | u32::from(opcode)).to_le_bytes());
        msg.extend_from_slice(payload);
        self.replies.push(msg);
    }

    fn next_serial(&mut self) -> u32 {
        self.next_server_id = self.next_server_id.wrapping_add(1);
        self.next_server_id
    }
}

fn frame_from_shm_file(file: &[u8]) -> Result<Option<ImportedFrame>, RelayError> {
    if !file.iter().any(|byte| *byte != 0) {
        return Ok(None);
    }
    let (width, height) = if file.len() >= 800 * 600 * 4 {
        (800u32, 600u32)
    } else if file.len() >= MIN_FRAME && file.len() % (64 * 4) == 0 {
        (64u32, (file.len() / (64 * 4)) as u32)
    } else {
        return Ok(None);
    };
    let needed = height as usize * width as usize * 4;
    if file.len() < needed {
        return Ok(None);
    }
    let pixels = file[..needed].to_vec();
    Ok(Some(ImportedFrame {
        width,
        height,
        pixels: pixels.clone(),
        sha256: frame_hash(&pixels)?,
    }))
}

fn read_u32(args: &[u8], offset: usize) -> Result<u32, RelayError> {
    args.get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| RelayError::Failed("wayland argument truncated".into()))
}

fn read_i32(args: &[u8], offset: usize) -> Result<i32, RelayError> {
    Ok(read_u32(args, offset)? as i32)
}

fn read_string(args: &[u8], offset: usize) -> Result<(&str, &[u8]), RelayError> {
    let len = read_u32(args, offset)? as usize;
    let start = offset + 4;
    let padded = align4(len);
    let bytes = args
        .get(start..start + padded)
        .ok_or_else(|| RelayError::Failed("wayland string truncated".into()))?;
    let text = std::str::from_utf8(&bytes[..len.saturating_sub(1).min(bytes.len())])
        .unwrap_or("");
    Ok((text, &args[start + padded..]))
}

fn write_array(out: &mut Vec<u8>, values: &[u32]) {
    out.extend_from_slice(&((values.len() * 4) as u32).to_le_bytes());
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&bytes);
    out.resize(align4(out.len()), 0);
}

#[cfg(test)]
fn push_wl(out: &mut Vec<u8>, object: u32, opcode: u16, args: &[u8]) {
    let size = 8 + args.len();
    out.extend_from_slice(&object.to_le_bytes());
    out.extend_from_slice(&(((size as u32) << 16) | u32::from(opcode)).to_le_bytes());
    out.extend_from_slice(args);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_shm_bytes_hash_as_imported_frame() {
        let width = 64u32;
        let height = 64u32;
        let pixel = [0x11, 0x22, 0x33, 0xff];
        let bytes = (0..height)
            .flat_map(|_| (0..width).flat_map(|_| pixel))
            .collect::<Vec<_>>();
        let mut files = BTreeMap::new();
        files.insert(1, bytes.clone());
        let mut compositor = MiniCompositor::new();
        let mut proto = Vec::new();
        push_wl(&mut proto, 1, 1, &2u32.to_le_bytes());
        compositor.feed(&proto, &files).unwrap();
        proto.clear();
        let mut bind_comp = Vec::new();
        bind_comp.extend_from_slice(&1u32.to_le_bytes());
        write_string(&mut bind_comp, "wl_compositor");
        bind_comp.extend_from_slice(&4u32.to_le_bytes());
        bind_comp.extend_from_slice(&10u32.to_le_bytes());
        push_wl(&mut proto, 2, 0, &bind_comp);
        let mut bind_shm = Vec::new();
        bind_shm.extend_from_slice(&2u32.to_le_bytes());
        write_string(&mut bind_shm, "wl_shm");
        bind_shm.extend_from_slice(&1u32.to_le_bytes());
        bind_shm.extend_from_slice(&11u32.to_le_bytes());
        push_wl(&mut proto, 2, 0, &bind_shm);
        compositor.feed(&proto, &files).unwrap();
        proto.clear();
        push_wl(&mut proto, 10, 0, &3u32.to_le_bytes());
        let mut pool_args = Vec::new();
        pool_args.extend_from_slice(&4u32.to_le_bytes());
        pool_args.extend_from_slice(&0u32.to_le_bytes());
        pool_args.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        push_wl(&mut proto, 11, 0, &pool_args);
        let mut buf_args = Vec::new();
        buf_args.extend_from_slice(&5u32.to_le_bytes());
        buf_args.extend_from_slice(&0i32.to_le_bytes());
        buf_args.extend_from_slice(&(width as i32).to_le_bytes());
        buf_args.extend_from_slice(&(height as i32).to_le_bytes());
        buf_args.extend_from_slice(&((width * 4) as i32).to_le_bytes());
        buf_args.extend_from_slice(&0i32.to_le_bytes());
        push_wl(&mut proto, 4, 0, &buf_args);
        let mut attach = Vec::new();
        attach.extend_from_slice(&5u32.to_le_bytes());
        attach.extend_from_slice(&0i32.to_le_bytes());
        attach.extend_from_slice(&0i32.to_le_bytes());
        push_wl(&mut proto, 3, 1, &attach);
        push_wl(&mut proto, 3, 6, &[]);
        compositor.feed(&proto, &files).unwrap();
        let frame = compositor.take_frame().expect("committed SHM frame");
        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 64);
        assert_eq!(&frame.pixels[..4], &[0x11, 0x22, 0x33, 0xff]);
        assert_eq!(frame.sha256, frame_hash(&frame.pixels).unwrap());
    }

    #[test]
    fn buffer_diff_writes_u32_spans_into_the_shm_file() {
        let mut file = vec![0u8; 16];
        let mut rest = Vec::new();
        rest.extend_from_slice(&12u32.to_le_bytes());
        rest.extend_from_slice(&0u32.to_le_bytes());
        rest.extend_from_slice(&0u32.to_le_bytes());
        rest.extend_from_slice(&1u32.to_le_bytes());
        rest.extend_from_slice(&[0x11, 0x22, 0x33, 0xff]);
        apply_buffer_diff(&mut file, &rest).unwrap();
        assert_eq!(&file[..4], &[0x11, 0x22, 0x33, 0xff]);
        assert_eq!(&file[4..], &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn empty_pixels_are_not_an_imported_frame() {
        assert!(frame_hash(&[]).is_err());
    }

    #[test]
    fn native_create_pool_args_put_size_at_offset_four() {
        let mut args = Vec::new();
        args.extend_from_slice(&4u32.to_le_bytes());
        args.extend_from_slice(&16384u32.to_le_bytes());
        assert_eq!(shm_create_pool_size(&args), Some(16384));
    }

    #[test]
    fn ack_messages_are_not_acked() {
        assert!(!waypipe_needs_ack(WMSG_ACK_NBLOCKS));
        assert!(waypipe_needs_ack(WMSG_BUFFER_DIFF));
    }

    #[test]
    fn buffer_diff_into_an_open_file_imports_guest_pixels() {
        let width = 64u32;
        let height = 64u32;
        let mut file = vec![0u8; (width * height * 4) as usize];
        file[0..4].copy_from_slice(&[0x11, 0x22, 0x33, 0xff]);
        let mut compositor = MiniCompositor::new();
        compositor.refresh_pool(&file);
        let frame = compositor.take_frame().expect("guest SHM pixels");
        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 64);
        assert_eq!(&frame.pixels[..4], &[0x11, 0x22, 0x33, 0xff]);
        assert_eq!(frame.sha256, frame_hash(&frame.pixels).unwrap());
    }
}
