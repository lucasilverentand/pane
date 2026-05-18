// swift-tools-version: 6.0
import PackageDescription

#if TUIST
import ProjectDescription

let packageSettings = PackageSettings(
    baseSettings: .settings(
        configurations: [
            .debug(name: .debug),
            .release(name: .release),
        ]
    )
)
#endif

let package = Package(
    name: "PaneDependencies",
    platforms: [.macOS(.v15)],
    dependencies: [
        .package(url: "https://github.com/CodeEditApp/CodeEditSourceEditor.git", from: "0.8.0"),
    ]
)
