//! WASI Preview 2 component hello.
//! cargo build --target wasm32-wasip2 --release

fn main() {
    println!("hello from wawona wasip2");
    println!("argv = {:?}", std::env::args().collect::<Vec<_>>());
}
