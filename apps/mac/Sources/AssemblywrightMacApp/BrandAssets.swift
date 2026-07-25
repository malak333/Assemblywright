import AppKit
import SwiftUI

/// Brand art that the packaged bundle carries in Contents/Resources.
///
/// `scripts/package-distribution.sh` copies these from assets/brand/generated.
/// A development run through `swift run` has no bundle resources, so every
/// lookup here is optional and callers fall back to an SF Symbol.
enum AssemblywrightBrandAssets {
    static let menuBarTemplateName = "menubar-template"

    /// The proofmark, marked as template art so AppKit tints it for the current
    /// menu bar appearance instead of drawing the raw black pixels.
    ///
    /// `NSImage(named:)` resolves the `@2x` and `@3x` companions from the same
    /// bundle, so the menu bar gets the representation matching its display.
    static func menuBarTemplate() -> NSImage? {
        guard let image = NSImage(named: menuBarTemplateName) else { return nil }
        image.isTemplate = true
        return image
    }
}

/// The menu bar label: the proofmark, plus a state badge when the core needs
/// attention. A healthy core shows the mark alone.
struct AssemblywrightMenuBarLabel: View {
    let presentation: AssemblywrightMenuBarPresentation

    var body: some View {
        if let template = AssemblywrightBrandAssets.menuBarTemplate() {
            HStack(spacing: 2) {
                Image(nsImage: template)
                if presentation.showsStateBadge {
                    Image(systemName: presentation.systemImage)
                }
            }
            .accessibilityLabel(accessibilityLabel)
        } else {
            Label(AssemblywrightMenuBarContract.title, systemImage: presentation.systemImage)
        }
    }

    private var accessibilityLabel: String {
        "\(AssemblywrightMenuBarContract.title), \(presentation.statusLine)"
    }
}
