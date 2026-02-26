// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "Pane",
    platforms: [
        .macOS(.v26),
        .iOS(.v26),
    ],
    products: [
        .library(name: "PaneKit", targets: ["PaneKit"]),
    ],
    dependencies: [
        .package(url: "https://github.com/migueldeicaza/SwiftTerm.git", from: "1.2.0"),
    ],
    targets: [
        // Pure Swift library — no external dependencies
        .target(
            name: "PaneKit",
            path: "Sources/PaneKit"
        ),

        // macOS app
        .executableTarget(
            name: "Pane",
            dependencies: [
                "PaneKit",
                .product(name: "SwiftTerm", package: "SwiftTerm"),
            ],
            path: "Sources/Pane"
        ),

        // iOS app placeholder
        .executableTarget(
            name: "PaneMobile",
            dependencies: ["PaneKit"],
            path: "Sources/PaneMobile"
        ),

        // Tests
        .testTarget(
            name: "PaneKitTests",
            dependencies: ["PaneKit"],
            path: "Tests/PaneKitTests"
        ),
    ]
)
