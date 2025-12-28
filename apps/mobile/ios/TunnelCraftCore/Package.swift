// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "TunnelCraftCore",
    platforms: [
        .iOS(.v15)
    ],
    products: [
        .library(
            name: "TunnelCraftCore",
            targets: ["TunnelCraftCore"]
        ),
    ],
    targets: [
        // C target for FFI declarations (headers/modulemap)
        .target(
            name: "TunnelCraftFFI",
            path: "Sources/TunnelCraftFFI",
            publicHeadersPath: "include"
        ),
        // Swift wrapper around the UniFFI bindings
        .target(
            name: "TunnelCraftCore",
            dependencies: [
                "TunnelCraftFFI",
                "TunnelCraftUniFFI"
            ],
            path: "Sources",
            exclude: ["TunnelCraftFFI", "Generated/tunnelcraft_uniffiFFI.h", "Generated/tunnelcraft_uniffiFFI.modulemap"],
            sources: ["TunnelCraftCore", "Generated"],
            linkerSettings: [
                // Required by Rust networking crates
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("Security"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("Network"),
            ]
        ),
        // Binary target for the Rust library
        .binaryTarget(
            name: "TunnelCraftUniFFI",
            path: "Frameworks/TunnelCraftUniFFI.xcframework"
        ),
    ]
)
