import ProjectDescription

let project = Project(
    name: "Pane",
    settings: .settings(
        base: [
            "DEVELOPMENT_TEAM": "96452FLT2P",
            "CODE_SIGN_STYLE": "Automatic",
            "ENABLE_USER_SCRIPT_SANDBOXING": "YES",
        ]
    ),
    targets: [
        // MARK: - PaneKit (pure Swift library, no external dependencies)
        .target(
            name: "PaneKit",
            destinations: [.mac, .iPhone, .iPad],
            product: .framework,
            bundleId: "studio.seventwo.pane.kit",
            deploymentTargets: .multiplatform(iOS: "26.0", macOS: "26.0"),
            sources: ["Sources/PaneKit/**"],
            dependencies: []
        ),

        // MARK: - Pane (macOS app)
        .target(
            name: "Pane",
            destinations: [.mac],
            product: .app,
            bundleId: "studio.seventwo.pane",
            deploymentTargets: .macOS("26.0"),
            infoPlist: .extendingDefault(with: [
                "CFBundleDisplayName": "Pane",
                "LSApplicationCategoryType": "public.app-category.developer-tools",
            ]),
            sources: ["Sources/Pane/**"],
            dependencies: [
                .target(name: "PaneKit"),
                .xcframework(path: "Frameworks/GhosttyKit.xcframework"),
                .sdk(name: "z", type: .library),
                .sdk(name: "c++", type: .library),
                .sdk(name: "Carbon", type: .framework),
            ]
        ),

        // MARK: - PaneBrowserMCP (stdio bridge CLI)
        .target(
            name: "PaneBrowserMCP",
            destinations: [.mac],
            product: .commandLineTool,
            bundleId: "studio.seventwo.pane.browser-mcp",
            deploymentTargets: .macOS("26.0"),
            sources: ["Sources/PaneBrowserMCP/**"],
            dependencies: [
                .target(name: "PaneKit"),
            ]
        ),

        // MARK: - PaneMobile (iOS app placeholder)
        .target(
            name: "PaneMobile",
            destinations: [.iPhone, .iPad],
            product: .app,
            bundleId: "studio.seventwo.pane.mobile",
            deploymentTargets: .iOS("26.0"),
            infoPlist: .extendingDefault(with: [
                "CFBundleDisplayName": "Pane",
            ]),
            sources: ["Sources/PaneMobile/**"],
            dependencies: [
                .target(name: "PaneKit"),
            ]
        ),

        // MARK: - PaneKitTests
        .target(
            name: "PaneKitTests",
            destinations: [.mac],
            product: .unitTests,
            bundleId: "studio.seventwo.pane.kit-tests",
            deploymentTargets: .macOS("26.0"),
            sources: ["Tests/PaneKitTests/**"],
            dependencies: [
                .target(name: "PaneKit"),
            ]
        ),
    ],
    schemes: [
        .scheme(
            name: "Pane",
            buildAction: .buildAction(targets: ["Pane"]),
            testAction: .targets(["PaneKitTests"]),
            runAction: .runAction(executable: "Pane")
        ),
    ]
)
