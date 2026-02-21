// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "CraftNetCore",
    platforms: [
        .iOS(.v15)
    ],
    products: [
        .library(
            name: "CraftNetCore",
            targets: ["CraftNetCore"]
        ),
    ],
    targets: [
        // C target for FFI declarations (headers/modulemap)
        .target(
            name: "CraftNetFFI",
            path: "Sources/CraftNetFFI",
            publicHeadersPath: "include"
        ),
        // Swift wrapper around the UniFFI bindings
        .target(
            name: "CraftNetCore",
            dependencies: [
                "CraftNetFFI",
                "CraftNetUniFFI"
            ],
            path: "Sources",
            exclude: ["CraftNetFFI", "Generated/craftnet_uniffiFFI.h", "Generated/craftnet_uniffiFFI.modulemap"],
            sources: ["CraftNetCore", "Generated"],
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
            name: "CraftNetUniFFI",
            path: "Frameworks/CraftNetUniFFI.xcframework"
        ),
    ]
)
