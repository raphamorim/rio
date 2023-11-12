//
//  SidebarView.swift
//  Rio
//
//  Created by Raphael Amorim on 2023-11-12.
//

import SwiftUI

struct SidebarView: View {
    let userCreatedGroups: [PanelGroup]
    @Binding var selection: PanelSection
//    @State private var selection = PanelSection.all
    
    var body: some View {
        List(selection: $selection) {
            Section("Work") {
                ForEach(PanelSection.allCases) {
                    selection in Label(selection.displayName, systemImage:  selection.iconName)
                        .tag(selection)
                }
            }
            
            Section("Personal") {
                ForEach(userCreatedGroups) {
                    group in Label(group.title, systemImage:  "folder")
                        .tag(PanelSection.list(group))
                }
            }
        }
    }
}

#Preview {
    SidebarView(userCreatedGroups: PanelGroup.examples(), selection: .constant(.all))
        .listStyle(.sidebar)
}
