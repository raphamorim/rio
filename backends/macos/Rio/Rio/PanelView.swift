//
//  PanelView.swift
//  Rio
//
//  Created by Raphael Amorim on 2023-11-12.
//

import SwiftUI

struct PanelView: View {
    let title: String
    let panels: [Panel]
    
    var body: some View {
        List(panels) { panel in
            HStack {
                Image(systemName: "circle")
                Text(panel.title)
            }
        }
    }
}

#Preview {
    PanelView(title: "work", panels: Panel.examples())
}
