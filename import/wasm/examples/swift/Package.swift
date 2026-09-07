// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "wasm-demo-swift",
    products: [
        .executable(name: "wasm-demo-swift", targets: ["wasm-demo-swift"]),
    ],
    targets: [
        .executableTarget(name: "wasm-demo-swift"),
    ]
)
