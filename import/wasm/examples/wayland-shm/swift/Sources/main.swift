// Colored rectangle via real Wayland (wl_shm + xdg).
// Build: ./build.sh  (swift.org wasm SDK — no Nix, no Foundation)

@_extern(wasm, module: "env", name: "wawona_wayland_connect")
func wawona_wayland_connect(_ fdOut: UnsafeMutablePointer<Int32>) -> Int32

@_extern(wasm, module: "env", name: "wawona_wayland_shm_create")
func wawona_wayland_shm_create(_ size: Int32, _ fdOut: UnsafeMutablePointer<Int32>) -> Int32

@_extern(wasm, module: "env", name: "wawona_wayland_shm_write")
func wawona_wayland_shm_write(_ shmFd: Int32, _ offset: Int32, _ buf: UnsafePointer<UInt8>, _ len: Int32) -> Int32

@_extern(wasm, module: "env", name: "wawona_wayland_sendmsg")
func wawona_wayland_sendmsg(_ wlFd: Int32, _ buf: UnsafePointer<UInt8>, _ len: Int32, _ scmFd: Int32) -> Int32

@_extern(wasm, module: "env", name: "wawona_socket_recv")
func wawona_socket_recv(_ fd: Int32, _ buf: UnsafeMutablePointer<UInt8>, _ len: Int32, _ nOut: UnsafeMutablePointer<Int32>) -> Int32

let display: UInt32 = 1
let width: Int32 = 256
let height: Int32 = 256
let stride: Int32 = width * 4
let formatXRGB: UInt32 = 1
let pixel: UInt32 = 0xFF3366CC

@main
struct WaylandShm {
    static func main() {
        var wl: Int32 = 0
        let rc = wawona_wayland_connect(&wl)
        if rc != 0 {
            print("wayland connect errno=\(rc) (is WAYLAND_DISPLAY set?)")
            return
        }
        var c = Conn(fd: wl)
        c.registry = c.alloc()
        c.req(display, 1, u32(c.registry))
        let cb = c.alloc()
        c.req(display, 0, u32(cb))
        c.roundtrip(cb)

        if c.compositor == 0 || c.shm == 0 || c.xdgWm == 0 {
            print("missing globals compositor=\(c.compositor) shm=\(c.shm) xdg=\(c.xdgWm)")
            return
        }

        c.surface = c.alloc()
        c.req(c.compositor, 0, u32(c.surface))
        c.xdgSurface = c.alloc()
        c.req(c.xdgWm, 2, u32(c.xdgSurface) + u32(c.surface))
        c.toplevel = c.alloc()
        c.req(c.xdgSurface, 1, u32(c.toplevel))
        c.setTitle("wawona-wasm-shm")
        c.req(c.surface, 6, [])

        let shmBytes = stride * height
        var shmFd: Int32 = 0
        if wawona_wayland_shm_create(shmBytes, &shmFd) != 0 {
            print("shm_create failed")
            return
        }
        var pixels = [UInt8](repeating: 0, count: Int(shmBytes))
        let px = u32(pixel)
        var i = 0
        while i < pixels.count {
            pixels[i] = px[0]; pixels[i + 1] = px[1]
            pixels[i + 2] = px[2]; pixels[i + 3] = px[3]
            i += 4
        }
        if pixels.withUnsafeBufferPointer({ buf in
            wawona_wayland_shm_write(shmFd, 0, buf.baseAddress!, Int32(pixels.count))
        }) != 0 {
            print("shm_write failed")
            return
        }

        c.pool = c.alloc()
        var poolMsg = header(c.shm, 0, 16)
        poolMsg += u32(c.pool)
        poolMsg += i32le(shmBytes)
        c.sendmsg(poolMsg, shmFd)

        c.buffer = c.alloc()
        var bufBody = u32(c.buffer)
        bufBody += i32le(0)
        bufBody += i32le(width)
        bufBody += i32le(height)
        bufBody += i32le(stride)
        bufBody += u32(formatXRGB)
        c.req(c.pool, 0, bufBody)

        while !c.configured && !c.closed {
            if !c.recvOnce() { break }
        }
        if c.closed { return }

        let attach = u32(c.buffer) + i32le(0) + i32le(0)
        c.req(c.surface, 1, attach)
        let damage = i32le(0) + i32le(0) + i32le(width) + i32le(height)
        c.req(c.surface, 2, damage)
        c.req(c.surface, 6, [])
        print("wayland-shm: 256x256 XRGB8888 committed (xdg)")

        while !c.closed {
            if !c.recvOnce() { break }
        }
    }
}

struct Conn {
    var fd: Int32
    var nextID: UInt32 = 2
    var registry: UInt32 = 0
    var compositor: UInt32 = 0
    var shm: UInt32 = 0
    var xdgWm: UInt32 = 0
    var surface: UInt32 = 0
    var xdgSurface: UInt32 = 0
    var toplevel: UInt32 = 0
    var pool: UInt32 = 0
    var buffer: UInt32 = 0
    var configured = false
    var closed = false

    mutating func alloc() -> UInt32 {
        let id = nextID
        nextID += 1
        return id
    }

    func req(_ obj: UInt32, _ opcode: UInt16, _ body: [UInt8]) {
        var msg = header(obj, opcode, UInt32(8 + body.count))
        msg += body
        sendmsg(msg, -1)
    }

    func sendmsg(_ msg: [UInt8], _ scm: Int32) {
        let rc = msg.withUnsafeBufferPointer { buf in
            wawona_wayland_sendmsg(fd, buf.baseAddress!, Int32(msg.count), scm)
        }
        if rc != 0 {
            print("sendmsg errno=\(rc)")
        }
    }

    func setTitle(_ title: String) {
        req(toplevel, 2, wlString(title))
    }

    mutating func roundtrip(_ cb: UInt32) {
        while true {
            guard let ev = readEvent() else { return }
            if ev.obj == cb && ev.opcode == 0 { return }
            if ev.obj == registry && ev.opcode == 0 { onGlobal(ev.payload) }
        }
    }

    mutating func recvOnce() -> Bool {
        guard let ev = readEvent() else { return false }
        if ev.obj == xdgWm && ev.opcode == 0 && ev.payload.count >= 4 {
            req(xdgWm, 3, Array(ev.payload[0..<4]))
        } else if ev.obj == xdgSurface && ev.opcode == 0 && ev.payload.count >= 4 {
            req(xdgSurface, 4, Array(ev.payload[0..<4]))
            configured = true
        } else if ev.obj == toplevel && ev.opcode == 1 {
            closed = true
        }
        return true
    }

    mutating func onGlobal(_ payload: [UInt8]) {
        if payload.count < 8 { return }
        let name = readU32(payload, 0)
        let (iface, restOff) = takeString(payload, 4)
        if payload.count < restOff + 4 { return }
        let version = readU32(payload, restOff)
        var want: UInt32 = 0
        var ver: UInt32 = 0
        if iface == "wl_compositor" {
            compositor = alloc()
            want = compositor
            ver = min(version, 4)
        } else if iface == "wl_shm" {
            shm = alloc()
            want = shm
            ver = 1
        } else if iface == "xdg_wm_base" {
            xdgWm = alloc()
            want = xdgWm
            ver = min(version, 2)
        } else {
            return
        }
        let body = u32(name) + wlString(iface) + u32(ver) + u32(want)
        req(registry, 0, body)
    }

    func readEvent() -> (obj: UInt32, opcode: UInt16, payload: [UInt8])? {
        var hdr = [UInt8](repeating: 0, count: 8)
        if !recvExact(fd, &hdr) { return nil }
        let obj = readU32(hdr, 0)
        let sizeOp = readU32(hdr, 4)
        let size = Int(sizeOp >> 16)
        let opcode = UInt16(sizeOp & 0xffff)
        if size < 8 { return nil }
        var payload = [UInt8](repeating: 0, count: size - 8)
        if !payload.isEmpty && !recvExact(fd, &payload) { return nil }
        return (obj, opcode, payload)
    }
}

func header(_ obj: UInt32, _ opcode: UInt16, _ size: UInt32) -> [UInt8] {
    u32(obj) + u32((size << 16) | UInt32(opcode))
}

func u32(_ v: UInt32) -> [UInt8] {
    [UInt8(v & 0xff), UInt8((v >> 8) & 0xff), UInt8((v >> 16) & 0xff), UInt8((v >> 24) & 0xff)]
}

func i32le(_ v: Int32) -> [UInt8] { u32(UInt32(bitPattern: v)) }

func wlString(_ s: String) -> [UInt8] {
    let bytes = Array(s.utf8)
    let n = bytes.count + 1
    var out = u32(UInt32(n))
    out += bytes
    out.append(0)
    while out.count % 4 != 0 { out.append(0) }
    return out
}

func takeString(_ data: [UInt8], _ off: Int) -> (String, Int) {
    if data.count < off + 4 { return ("", off) }
    let n = Int(readU32(data, off))
    let padded = ((n + 3) / 4) * 4
    if data.count < off + 4 + padded { return ("", off) }
    let raw = data[(off + 4)..<(off + 4 + max(n - 1, 0))]
    return (String(decoding: raw, as: UTF8.self), off + 4 + padded)
}

func readU32(_ data: [UInt8], _ off: Int) -> UInt32 {
    UInt32(data[off])
        | UInt32(data[off + 1]) << 8
        | UInt32(data[off + 2]) << 16
        | UInt32(data[off + 3]) << 24
}

func recvExact(_ fd: Int32, _ dest: inout [UInt8]) -> Bool {
    var off = 0
    while off < dest.count {
        var n: Int32 = 0
        let remaining = Int32(dest.count - off)
        let rc = dest.withUnsafeMutableBufferPointer { buf in
            wawona_socket_recv(fd, buf.baseAddress!.advanced(by: off), remaining, &n)
        }
        if rc != 0 || n <= 0 { return false }
        off += Int(n)
    }
    return true
}

func min(_ a: UInt32, _ b: UInt32) -> UInt32 { a < b ? a : b }
