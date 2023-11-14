//
//  PanelSection.swift
//  Rio
//
//  Created by Raphael Amorim on 2023-11-12.
//

import Foundation

enum PanelSection: Identifiable, CaseIterable, Hashable {
    case all
    case done
    case upcoming
    case list(PanelGroup)
    
    var id: String {
        switch self {
            case .all:
                "all"
            case .done:
                "done"
            case .upcoming:
                "upcoming"
            case .list(let panelGroup):
                panelGroup.id.uuidString
        }
    }
    
    var displayName: String {
        switch self {
            case .all:
                "All"
            case .done:
                "Done"
            case .upcoming:
                "Upcoming"
            case .list(let panelGroup):
                panelGroup.title
        }
    }
    
    var iconName: String {
        switch self {
            case .all:
                "star"
            case .done:
                "checkmark.circle"
            case .upcoming:
                "calendar"
            case .list(_):
                "folder"
        }
    }
    
    static var allCases: [PanelSection] {
        [.all, .done, .upcoming]
    }
    
    static func == (lhs: PanelSection, rhs: PanelSection) -> Bool {
        lhs.id == rhs.id
    }
}
