// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "AssemblywrightMac",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .library(name: "AssemblywrightMacCore", targets: ["JarvisMacCore"]),
        .executable(name: "AssemblywrightMacApp", targets: ["JarvisMacApp"]),
        .executable(name: "assemblywright-mac-bridge", targets: ["JarvisMacBridgeCLI"]),
        // Legacy product aliases remain available during the compatibility window.
        .library(name: "JarvisMacCore", targets: ["JarvisMacCore"]),
        .executable(name: "JarvisMacApp", targets: ["JarvisMacApp"]),
        .executable(name: "jarvis-mac-bridge", targets: ["JarvisMacBridgeCLI"])
    ],
    targets: [
        .target(
            name: "JarvisMacCore",
            linkerSettings: [.linkedFramework("Security")]
        ),
        .executableTarget(
            name: "JarvisMacApp",
            dependencies: ["JarvisMacCore"]
        ),
        .executableTarget(
            name: "JarvisMacBridgeCLI",
            dependencies: ["JarvisMacCore"]
        ),
        .testTarget(
            name: "JarvisMacCoreTests",
            dependencies: ["JarvisMacCore"]
        ),
        .testTarget(
            name: "JarvisMacAppTests",
            dependencies: ["JarvisMacApp", "JarvisMacCore"]
        )
    ]
)
