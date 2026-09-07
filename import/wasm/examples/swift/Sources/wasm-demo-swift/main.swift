// WASI P1 demo. Build with the swift.org wasm SDK (wasm32-unknown-wasip1).
// No Foundation — keeps the module small enough to instantiate on iPhone.

@main
struct Demo {
    static func main() {
        let args = CommandLine.arguments
        let cmd = args.count > 1 ? args[1] : "help"
        switch cmd {
        case "hello":
            print("hello from wawona wasm-demo-swift")
            print("argv = \(args)")
        default:
            print("usage: wasm-demo-swift hello")
        }
    }
}
