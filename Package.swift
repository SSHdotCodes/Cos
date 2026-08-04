// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "Cos",
    platforms: [.macOS(.v15)],
    products: [
        .library(name: "CosCore", targets: ["CosCore"]),
        .executable(name: "Cos", targets: ["Cos"]),
    ],
    targets: [
        .target(
            name: "CosCore",
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("ApplicationServices"),
                .linkedFramework("Security"),
            ]
        ),
        .executableTarget(
            name: "Cos",
            dependencies: ["CosCore"],
            resources: [
                .copy("Resources/BuiltInPlugins"),
                .copy("Resources/ProviderLogos"),
            ],
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("WebKit"),
            ]
        ),
        .testTarget(name: "CosCoreTests", dependencies: ["CosCore"]),
    ],
    swiftLanguageModes: [.v5]
)
