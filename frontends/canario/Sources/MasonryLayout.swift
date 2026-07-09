import SwiftUI

struct PanelGridLayout: Layout {
    var spacing: CGFloat = 8

    struct ColumnKey: LayoutValueKey {
        static let defaultValue: Int = 0
    }

    struct WeightKey: LayoutValueKey {
        static let defaultValue: CGFloat = 1
    }

    func sizeThatFits(
        proposal: ProposedViewSize, subviews: Subviews, cache: inout ()
    ) -> CGSize {
        proposal.replacingUnspecifiedDimensions()
    }

    func placeSubviews(
        in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()
    ) {
        guard !subviews.isEmpty else { return }

        var columnMembers: [Int: [Int]] = [:]
        for index in subviews.indices {
            columnMembers[subviews[index][ColumnKey.self], default: []].append(index)
        }
        let columnKeys = columnMembers.keys.sorted()
        let columnCount = columnKeys.count
        let columnWidth =
            (bounds.width - CGFloat(columnCount - 1) * spacing) / CGFloat(columnCount)

        for (columnIndex, key) in columnKeys.enumerated() {
            let indices = columnMembers[key] ?? []
            guard !indices.isEmpty else { continue }
            let totalWeight = indices.reduce(CGFloat(0)) {
                $0 + subviews[$1][WeightKey.self]
            }
            let availableHeight = bounds.height - CGFloat(indices.count - 1) * spacing
            let x = bounds.minX + CGFloat(columnIndex) * (columnWidth + spacing)
            var y = bounds.minY
            for index in indices {
                let height = availableHeight * subviews[index][WeightKey.self] / totalWeight
                subviews[index].place(
                    at: CGPoint(x: x, y: y),
                    anchor: .topLeading,
                    proposal: ProposedViewSize(width: columnWidth, height: height))
                y += height + spacing
            }
        }
    }
}
