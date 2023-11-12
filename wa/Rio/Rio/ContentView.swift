//
//  ContentView.swift
//  Rio
//
//  Created by Raphael Amorim on 2023-11-12.
//

import SwiftUI

struct ContentView: View {
    @State private var allPanels = Panel.examples()
    @State private var selection: PanelSection = PanelSection.all
    @State private var userCreatedGroups: [PanelGroup] = PanelGroup.examples()
    
    var body: some View {
        NavigationSplitView {
            SidebarView(userCreatedGroups: userCreatedGroups, selection: $selection)
        } detail: {
            
            switch selection {
                case .all:
                    PanelView(title: "All", panels: allPanels)
                case .done:
                    PanelView(title: "Done", panels: allPanels)
                case .upcoming:
                    PanelView(title: "Upcoming", panels: allPanels)
                case .list(let panelGroup):
                    PanelView(title: panelGroup.title, panels: panelGroup.panels)
            }
        }
    }
}

#Preview {
    ContentView()
}
