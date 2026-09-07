// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "wayland-shm",
    products: [
        .executable(name: "wayland-shm", targets: ["WaylandShm"]),
    ],
    targets: [
        .executableTarget(
            name: "WaylandShm",
            path: "Sources",
            swiftSettings: [
                .enableExperimentalFeature("Extern"),
            ]
        ),
    ]
)
