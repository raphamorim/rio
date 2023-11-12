//
//  PanelGroup.swift
//  Rio
//
//  Created by Raphael Amorim on 2023-11-12.
//

import Foundation

struct PanelGroup: Identifiable, Hashable {
    let id = UUID()
    var title: String
    var panels: [Panel]
    
    init(title: String, panels: [Panel] = []) {
        self.title = title
        self.panels = panels
    }
    
    static func example() -> PanelGroup {
        let panel1 = Panel(title: "zsh")
        let panel2 = Panel(title: "nvim")
        let panel3 = Panel(title: "nvim")
        
        var group = PanelGroup(title: "oi")
        group.panels = [panel1, panel2, panel3]
        return group
    }
    
    static func examples() -> [PanelGroup] {
        let group1 = PanelGroup.example()
        let group2 = PanelGroup(title: "oi")
        return [group1, group2]
    }
}
