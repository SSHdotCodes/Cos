#!/usr/bin/env swift

import AppKit
import Foundation

let arguments = CommandLine.arguments
guard arguments.count == 2 else {
    FileHandle.standardError.write(Data("usage: generate_icon.swift OUTPUT_DIRECTORY\n".utf8))
    exit(64)
}

let output = URL(fileURLWithPath: arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)

let variants: [(String, Int)] = [
    ("icon_16x16.png", 16), ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32), ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128), ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256), ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512), ("icon_512x512@2x.png", 1024),
]

for (name, pixels) in variants {
    let size = NSSize(width: pixels, height: pixels)
    let image = NSImage(size: size)
    image.lockFocus()
    guard let context = NSGraphicsContext.current?.cgContext else { exit(1) }
    context.setAllowsAntialiasing(true)
    context.setShouldAntialias(true)

    let margin = CGFloat(pixels) * 0.035
    let rect = CGRect(x: margin, y: margin, width: CGFloat(pixels) - margin * 2, height: CGFloat(pixels) - margin * 2)
    let radius = CGFloat(pixels) * 0.225
    let path = CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
    context.saveGState()
    context.addPath(path)
    context.clip()

    context.setFillColor(NSColor.black.cgColor)
    context.fill(rect)
    context.restoreGState()

    let mainFontSize = CGFloat(pixels) * 0.245
    let mathFont = NSFont(name: "STIXTwoMath-Regular", size: mainFontSize)
        ?? NSFont(name: "TimesNewRomanPSMT", size: mainFontSize)
        ?? .systemFont(ofSize: mainFontSize, weight: .medium)
    let paragraph = NSMutableParagraphStyle()
    paragraph.alignment = .center
    let text = NSAttributedString(
        string: "cos θ",
        attributes: [
            .font: mathFont,
            .foregroundColor: NSColor.white,
            .paragraphStyle: paragraph,
            .kern: -CGFloat(pixels) * 0.004,
        ]
    )
    let textHeight = text.size().height
    let textRect = CGRect(
        x: rect.minX,
        y: rect.midY - textHeight * 0.50,
        width: rect.width,
        height: textHeight * 1.15
    )
    NSGraphicsContext.current?.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = .black.withAlphaComponent(0.3)
    shadow.shadowBlurRadius = CGFloat(pixels) * 0.02
    shadow.shadowOffset = NSSize(width: 0, height: -CGFloat(pixels) * 0.008)
    shadow.set()
    text.draw(in: textRect)
    NSGraphicsContext.current?.restoreGraphicsState()

    image.unlockFocus()
    guard let tiff = image.tiffRepresentation,
          let bitmap = NSBitmapImageRep(data: tiff),
          let png = bitmap.representation(using: .png, properties: [:]) else { exit(1) }
    try png.write(to: output.appendingPathComponent(name), options: .atomic)
}
