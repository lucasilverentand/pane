import ProjectDescription

let project = Project(
    name: "Pane",
    organizationName: "seventwo.studio",
    settings: .settings(
        base: [
            "DEVELOPMENT_TEAM": "V4M4LHV9Y7",
            "SWIFT_VERSION": "6.0",
        ],
        configurations: [
            .debug(name: .debug),
            .release(name: .release),
        ]
    ),
    targets: [
        .target(
            name: "Pane",
            destinations: .macOS,
            product: .app,
            bundleId: "studio.seventwo.pane",
            deploymentTargets: .macOS("15.0"),
            infoPlist: .extendingDefault(with: [
                "CFBundleDisplayName": "Pane",
                "CFBundleIconName": "AppIcon",
                "LSApplicationCategoryType": "public.app-category.developer-tools",
                "NSMainStoryboardFile": "",
            ]),
            sources: ["Sources/Pane/**"],
            resources: ["Sources/Pane/Resources/**"],
            dependencies: [
                .external(name: "CodeEditSourceEditor"),
            ]
        ),
    ]
)
