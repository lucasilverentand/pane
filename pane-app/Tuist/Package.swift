// swift-tools-version: 6.2

import PackageDescription

#if TUIST
import struct ProjectDescription.PackageSettings

let packageSettings = PackageSettings()
#endif

let package = Package(
    name: "PaneDependencies",
    dependencies: [
        .package(url: "https://github.com/migueldeicaza/SwiftTerm.git", from: "1.2.0"),
    ]
)
