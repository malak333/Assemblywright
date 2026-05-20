// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "JarvisMac",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .library(name: "JarvisMacCore", targets: ["JarvisMacCore"]),
        .executable(name: "JarvisMacApp", targets: ["JarvisMacApp"])
    ],
    targets: [
        .target(name: "JarvisMacCore"),
        .executableTarget(
            name: "JarvisMacApp",
            dependencies: ["JarvisMacCore"]
        ),
        .testTarget(
            name: "JarvisMacCoreTests",
            dependencies: ["JarvisMacCore"]
        )
    ]
)
