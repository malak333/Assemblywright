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
        .executable(name: "assemblywright-mac-bridge", targets: ["JarvisMacBridgeCLI"])
    ],
    // Target names stay `Jarvis*` because module names are internal and
    // renaming them would touch every import for no external benefit. The
    // built executable is named after the *product*, so packaging copies
    // `AssemblywrightMacApp` into the bundle as `Contents/MacOS/JarvisMacApp`.
    // That bundle-internal filename is a signed-identity and release-evidence
    // contract: signed provenance and live-device QA reports bind it directly.
    // See SWIFT_APP_PRODUCT in scripts/package-distribution.sh.
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
