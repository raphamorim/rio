//
//  Panel.swift
//  Rio
//
//  Created by Raphael Amorim on 2023-11-12.
//

import Foundation

struct Panel: Identifiable, Hashable {
    let id = UUID()
    var title: String
    
    init(title: String) {
        self.title = title
    }
    
    static func example() -> Panel {
        Panel(title: "zsh")
    }
    
    static func examples() -> [Panel] {
        [
            Panel(title: "zsh"),
            Panel(title: "vim"),
            Panel(title: "nvim"),
        ]
    }
}
