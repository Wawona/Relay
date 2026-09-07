// Colored rectangle via real Wayland (wl_shm + xdg). Build: ./build.sh
// (Go 1.21+ WASI — no Nix).
package main

import (
	"encoding/binary"
	"fmt"
	"os"
	"unsafe"
)

//go:wasmimport env wawona_wayland_connect
func wawona_wayland_connect(fdOut *int32) int32

//go:wasmimport env wawona_wayland_shm_create
func wawona_wayland_shm_create(size int32, fdOut *int32) int32

//go:wasmimport env wawona_wayland_shm_write
func wawona_wayland_shm_write(shmFd int32, offset int32, buf *byte, length int32) int32

//go:wasmimport env wawona_wayland_sendmsg
func wawona_wayland_sendmsg(wlFd int32, buf *byte, length int32, scmFd int32) int32

//go:wasmimport env wawona_socket_recv
func wawona_socket_recv(fd int32, buf *byte, length int32, nOut *int32) int32

const (
	display        = 1
	width          = 256
	height         = 256
	stride         = width * 4
	formatXRGB8888 = 1
	pixel          = 0xFF3366CC
)

type conn struct {
	wl, nextID                         uint32
	registry, compositor, shm, xdgWm   uint32
	surface, xdgSurface, toplevel      uint32
	pool, buffer                       uint32
	configured, closed                 bool
	fd                                 int32
}

func main() {
	var wl int32
	if rc := wawona_wayland_connect(&wl); rc != 0 {
		fmt.Fprintf(os.Stderr, "wayland connect errno=%d (is WAYLAND_DISPLAY set?)\n", rc)
		os.Exit(1)
	}
	c := &conn{fd: wl, nextID: 2}

	c.registry = c.alloc()
	c.req(display, 1, u32(c.registry))
	cb := c.alloc()
	c.req(display, 0, u32(cb))
	c.roundtrip(cb)

	if c.compositor == 0 || c.shm == 0 || c.xdgWm == 0 {
		fmt.Fprintf(os.Stderr, "missing globals compositor=%d shm=%d xdg=%d\n", c.compositor, c.shm, c.xdgWm)
		os.Exit(1)
	}

	c.surface = c.alloc()
	c.req(c.compositor, 0, u32(c.surface))
	c.xdgSurface = c.alloc()
	c.req(c.xdgWm, 2, append(u32(c.xdgSurface), u32(c.surface)...))
	c.toplevel = c.alloc()
	c.req(c.xdgSurface, 1, u32(c.toplevel))
	c.setTitle("wawona-wasm-shm")
	c.req(c.surface, 6, nil)

	shmBytes := int32(stride * height)
	var shmFd int32
	if rc := wawona_wayland_shm_create(shmBytes, &shmFd); rc != 0 {
		fmt.Fprintf(os.Stderr, "shm_create errno=%d\n", rc)
		os.Exit(1)
	}
	pixels := make([]byte, shmBytes)
	for i := 0; i < len(pixels); i += 4 {
		binary.LittleEndian.PutUint32(pixels[i:], pixel)
	}
	if rc := wawona_wayland_shm_write(shmFd, 0, &pixels[0], int32(len(pixels))); rc != 0 {
		fmt.Fprintf(os.Stderr, "shm_write errno=%d\n", rc)
		os.Exit(1)
	}

	c.pool = c.alloc()
	poolMsg := header(c.shm, 0, 16)
	poolMsg = append(poolMsg, u32(c.pool)...)
	poolMsg = append(poolMsg, i32le(shmBytes)...)
	c.sendmsg(poolMsg, shmFd)

	c.buffer = c.alloc()
	var bufBody []byte
	bufBody = append(bufBody, u32(c.buffer)...)
	bufBody = append(bufBody, i32le(0)...)
	bufBody = append(bufBody, i32le(width)...)
	bufBody = append(bufBody, i32le(height)...)
	bufBody = append(bufBody, i32le(stride)...)
	bufBody = append(bufBody, u32(formatXRGB8888)...)
	c.req(c.pool, 0, bufBody)

	for !c.configured && !c.closed {
		if !c.recvOnce() {
			break
		}
	}
	if c.closed {
		return
	}

	var attach []byte
	attach = append(attach, u32(c.buffer)...)
	attach = append(attach, i32le(0)...)
	attach = append(attach, i32le(0)...)
	c.req(c.surface, 1, attach)
	var damage []byte
	damage = append(damage, i32le(0)...)
	damage = append(damage, i32le(0)...)
	damage = append(damage, i32le(width)...)
	damage = append(damage, i32le(height)...)
	c.req(c.surface, 2, damage)
	c.req(c.surface, 6, nil)
	fmt.Println("wayland-shm: 256x256 XRGB8888 committed (xdg)")

	for !c.closed {
		if !c.recvOnce() {
			break
		}
	}
}

func (c *conn) alloc() uint32 {
	id := c.nextID
	c.nextID++
	return id
}

func (c *conn) req(obj uint32, opcode uint16, body []byte) {
	msg := header(obj, opcode, uint32(8+len(body)))
	msg = append(msg, body...)
	c.sendmsg(msg, -1)
}

func (c *conn) sendmsg(msg []byte, scm int32) {
	if rc := wawona_wayland_sendmsg(c.fd, &msg[0], int32(len(msg)), scm); rc != 0 {
		fmt.Fprintf(os.Stderr, "sendmsg errno=%d\n", rc)
		os.Exit(1)
	}
}

func (c *conn) setTitle(title string) {
	c.req(c.toplevel, 2, wlString(title))
}

func (c *conn) roundtrip(cb uint32) {
	for {
		obj, opcode, payload, ok := c.readEvent()
		if !ok {
			return
		}
		if obj == cb && opcode == 0 {
			return
		}
		if obj == c.registry && opcode == 0 {
			c.onGlobal(payload)
		}
	}
}

func (c *conn) recvOnce() bool {
	obj, opcode, payload, ok := c.readEvent()
	if !ok {
		return false
	}
	if obj == c.xdgWm && opcode == 0 && len(payload) >= 4 {
		c.req(c.xdgWm, 3, payload[:4])
	} else if obj == c.xdgSurface && opcode == 0 && len(payload) >= 4 {
		c.req(c.xdgSurface, 4, payload[:4])
		c.configured = true
	} else if obj == c.toplevel && opcode == 1 {
		c.closed = true
	}
	return true
}

func (c *conn) onGlobal(payload []byte) {
	if len(payload) < 8 {
		return
	}
	name := binary.LittleEndian.Uint32(payload[0:4])
	iface, rest := takeString(payload[4:])
	if len(rest) < 4 {
		return
	}
	version := binary.LittleEndian.Uint32(rest[0:4])
	var want, ver uint32
	switch iface {
	case "wl_compositor":
		c.compositor = c.alloc()
		want, ver = c.compositor, min(version, 4)
	case "wl_shm":
		c.shm = c.alloc()
		want, ver = c.shm, 1
	case "xdg_wm_base":
		c.xdgWm = c.alloc()
		want, ver = c.xdgWm, min(version, 2)
	default:
		return
	}
	body := u32(name)
	body = append(body, wlString(iface)...)
	body = append(body, u32(ver)...)
	body = append(body, u32(want)...)
	c.req(c.registry, 0, body)
}

func (c *conn) readEvent() (uint32, uint16, []byte, bool) {
	hdr := make([]byte, 8)
	if !recvExact(c.fd, hdr) {
		return 0, 0, nil, false
	}
	obj := binary.LittleEndian.Uint32(hdr[0:4])
	sizeOp := binary.LittleEndian.Uint32(hdr[4:8])
	size := int(sizeOp >> 16)
	opcode := uint16(sizeOp & 0xffff)
	if size < 8 {
		return 0, 0, nil, false
	}
	payload := make([]byte, size-8)
	if len(payload) > 0 && !recvExact(c.fd, payload) {
		return 0, 0, nil, false
	}
	return obj, opcode, payload, true
}

func header(obj uint32, opcode uint16, size uint32) []byte {
	b := u32(obj)
	b = append(b, u32((size<<16)|uint32(opcode))...)
	return b
}

func u32(v uint32) []byte {
	var b [4]byte
	binary.LittleEndian.PutUint32(b[:], v)
	return b[:]
}

func i32le(v int32) []byte { return u32(uint32(v)) }

func wlString(s string) []byte {
	n := len(s) + 1
	b := u32(uint32(n))
	b = append(b, s...)
	b = append(b, 0)
	for len(b)%4 != 0 {
		b = append(b, 0)
	}
	return b
}

func takeString(data []byte) (string, []byte) {
	if len(data) < 4 {
		return "", data
	}
	n := int(binary.LittleEndian.Uint32(data[0:4]))
	padded := ((n + 3) / 4) * 4
	if len(data) < 4+padded {
		return "", data
	}
	raw := data[4 : 4+n]
	if n > 0 {
		raw = raw[:n-1]
	}
	return string(raw), data[4+padded:]
}

func recvExact(fd int32, dest []byte) bool {
	off := 0
	for off < len(dest) {
		var n int32
		rc := wawona_socket_recv(fd, &dest[off], int32(len(dest)-off), &n)
		if rc != 0 || n <= 0 {
			return false
		}
		off += int(n)
	}
	return true
}

func min(a, b uint32) uint32 {
	if a < b {
		return a
	}
	return b
}

// keep unsafe imported for wasm pointer passing on some Go versions
var _ = unsafe.Sizeof(int32(0))
