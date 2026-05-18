import AppKit
import CodeEditSourceEditor

extension EditorTheme {
    static var paneLight: EditorTheme {
        EditorTheme(
            text: Attribute(color: NSColor(srgbRed: 0.13, green: 0.13, blue: 0.13, alpha: 1)),
            insertionPoint: .controlAccentColor,
            invisibles: Attribute(color: .tertiaryLabelColor),
            background: NSColor(srgbRed: 0.98, green: 0.98, blue: 0.98, alpha: 1),
            lineHighlight: NSColor(srgbRed: 0.93, green: 0.95, blue: 1.0, alpha: 1),
            selection: NSColor.selectedTextBackgroundColor,
            keywords: Attribute(color: NSColor(srgbRed: 0.61, green: 0.14, blue: 0.58, alpha: 1), bold: true),
            commands: Attribute(color: NSColor(srgbRed: 0.20, green: 0.43, blue: 0.45, alpha: 1)),
            types: Attribute(color: NSColor(srgbRed: 0.04, green: 0.31, blue: 0.47, alpha: 1)),
            attributes: Attribute(color: NSColor(srgbRed: 0.51, green: 0.37, blue: 0.01, alpha: 1)),
            variables: Attribute(color: NSColor(srgbRed: 0.06, green: 0.41, blue: 0.63, alpha: 1)),
            values: Attribute(color: NSColor(srgbRed: 0.42, green: 0.21, blue: 0.66, alpha: 1)),
            numbers: Attribute(color: NSColor(srgbRed: 0.11, green: 0.00, blue: 0.81, alpha: 1)),
            strings: Attribute(color: NSColor(srgbRed: 0.77, green: 0.10, blue: 0.09, alpha: 1)),
            characters: Attribute(color: NSColor(srgbRed: 0.11, green: 0.00, blue: 0.81, alpha: 1)),
            comments: Attribute(color: NSColor(srgbRed: 0.15, green: 0.46, blue: 0.03, alpha: 1))
        )
    }

    static var paneDark: EditorTheme {
        EditorTheme(
            text: Attribute(color: NSColor(srgbRed: 0.92, green: 0.92, blue: 0.93, alpha: 1)),
            insertionPoint: .controlAccentColor,
            invisibles: Attribute(color: .tertiaryLabelColor),
            background: NSColor(srgbRed: 0.11, green: 0.11, blue: 0.12, alpha: 1),
            lineHighlight: NSColor(srgbRed: 0.14, green: 0.14, blue: 0.16, alpha: 1),
            selection: NSColor.selectedTextBackgroundColor,
            keywords: Attribute(color: NSColor(srgbRed: 1.0, green: 0.48, blue: 0.70, alpha: 1), bold: true),
            commands: Attribute(color: NSColor(srgbRed: 0.47, green: 0.76, blue: 0.70, alpha: 1)),
            types: Attribute(color: NSColor(srgbRed: 0.42, green: 0.87, blue: 1.0, alpha: 1)),
            attributes: Attribute(color: NSColor(srgbRed: 0.80, green: 0.59, blue: 0.41, alpha: 1)),
            variables: Attribute(color: NSColor(srgbRed: 0.31, green: 0.69, blue: 0.80, alpha: 1)),
            values: Attribute(color: NSColor(srgbRed: 0.70, green: 0.51, blue: 0.92, alpha: 1)),
            numbers: Attribute(color: NSColor(srgbRed: 0.85, green: 0.79, blue: 0.49, alpha: 1)),
            strings: Attribute(color: NSColor(srgbRed: 1.0, green: 0.51, blue: 0.44, alpha: 1)),
            characters: Attribute(color: NSColor(srgbRed: 0.85, green: 0.79, blue: 0.49, alpha: 1)),
            comments: Attribute(color: NSColor(srgbRed: 0.50, green: 0.55, blue: 0.60, alpha: 1))
        )
    }
}
