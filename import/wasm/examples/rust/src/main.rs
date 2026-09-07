//! WASI P1 demo: hello, fs-write, fs-read, fs-escape, tcp-client.
//! Build: rustup target add wasm32-wasip1 && cargo build --target wasm32-wasip1 --release

#[link(wasm_import_module = "wawona_socket")]
extern "C" {
    fn wawona_socket_socket(domain: i32, ty: i32, fd_out: *mut i32) -> i32;
    fn wawona_socket_connect_host(fd: i32, host: *const u8, host_len: i32, port: i32) -> i32;
    fn wawona_socket_send(fd: i32, buf: *const u8, len: i32, sent_out: *mut i32) -> i32;
    fn wawona_socket_close(fd: i32) -> i32;
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");
    match cmd {
        "hello" => {
            println!("hello from wawona wasm-demo");
            println!("argv = {args:?}");
            println!("HOME = {:?}", std::env::var("HOME").ok());
            println!("PWD = {:?}", std::env::var("PWD").ok());
        }
        "fs-write" => {
            let path = args.get(2).map(String::as_str).unwrap_or("demo.txt");
            std::fs::write(path, b"wawona wasm fs-write\n").expect("write");
            println!("wrote {path}");
        }
        "fs-read" => {
            let path = args.get(2).map(String::as_str).unwrap_or("demo.txt");
            let s = std::fs::read_to_string(path).expect("read");
            print!("{s}");
        }
        "fs-escape" => {
            match std::fs::read_to_string("../../outside.txt") {
                Ok(_) => {
                    eprintln!("FAIL: escape succeeded");
                    std::process::exit(2);
                }
                Err(e) => {
                    println!("fs-escape denied as expected: {e}");
                }
            }
        }
        "tcp-client" => {
            let host = args.get(2).map(String::as_str).unwrap_or("example.com");
            let port: i32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(80);
            let mut fd = 0i32;
            let rc = unsafe { wawona_socket_socket(2, 1, &mut fd) };
            if rc != 0 {
                eprintln!("socket errno={rc}");
                std::process::exit(1);
            }
            let rc = unsafe {
                wawona_socket_connect_host(fd, host.as_ptr(), host.len() as i32, port)
            };
            if rc != 0 {
                eprintln!("connect {host}:{port} errno={rc}");
                std::process::exit(1);
            }
            let req = format!("GET / HTTP/1.0\r\nHost: {host}\r\n\r\n");
            let mut sent = 0i32;
            let _ = unsafe {
                wawona_socket_send(fd, req.as_ptr(), req.len() as i32, &mut sent)
            };
            unsafe { wawona_socket_close(fd) };
            println!("tcp-client: sent {sent} bytes to {host}:{port}");
        }
        _ => {
            eprintln!("usage: wasm-demo hello|fs-write|fs-read|fs-escape|tcp-client");
            std::process::exit(1);
        }
    }
}
