// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "AssemblywrightMac",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .library(name: "AssemblywrightMacCore", targets: ["AssemblywrightMacCore"]),
        .executable(name: "AssemblywrightMacApp", targets: ["AssemblywrightMacApp"]),
        .executable(name: "assemblywright-mac-bridge", targets: ["AssemblywrightMacBridgeCLI"])
    ],
    // Products and targets share names, so the built executable and the
    // bundle-internal filename agree: packaging copies `AssemblywrightMacApp`
    // into `Contents/MacOS/AssemblywrightMacApp`. That filename is a
    // signed-identity and release-evidence contract — signed provenance and
    // live-device QA reports bind it directly — so changing it means
    // regenerating that evidence. See SWIFT_APP_PRODUCT and
    // APP_EXECUTABLE_NAME in scripts/package-distribution.sh.
    targets: [
        .target(
            name: "AssemblywrightMacCore",
            linkerSettings: [.linkedFramework("Security")]
        ),
        .executableTarget(
            name: "AssemblywrightMacApp",
            dependencies: ["AssemblywrightMacCore"]
        ),
        .executableTarget(
            name: "AssemblywrightMacBridgeCLI",
            dependencies: ["AssemblywrightMacCore"]
        ),
        .testTarget(
            name: "AssemblywrightMacCoreTests",
            dependencies: ["AssemblywrightMacCore"]
        ),
        .testTarget(
            name: "AssemblywrightMacAppTests",
            dependencies: ["AssemblywrightMacApp", "AssemblywrightMacCore"]
        )
    ]
)
