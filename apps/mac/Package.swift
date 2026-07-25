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
    // Target names stay `Jarvis*` on purpose. The target name determines the
    // built executable filename, and `JarvisMacApp` is recorded inside signed
    // provenance, live-device QA reports, and the installed bundle layout at
    // `Assemblywright.app/Contents/MacOS/JarvisMacApp`. Renaming it would change
    // the signed app-executable path that release evidence binds to.
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
