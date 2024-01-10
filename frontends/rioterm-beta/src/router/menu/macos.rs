use wa::native::apple::frameworks::{sel, sel_impl};
use wa::native::apple::menu::KeyAssignment;
use wa::native::apple::menu::Menu;
use wa::native::apple::menu::RepresentedItem;
use wa::native::apple::menu::SEL;
use wa::*;

pub fn create_menu() {
    let rio_perform_key_assignment_sel = sel!(rioPerformKeyAssignment:);

    fn mark_candidates(menu: &Menu, candidates: &mut Vec<MenuItem>, action: SEL) {
        for item in menu.items() {
            if let Some(submenu) = item.get_sub_menu() {
                mark_candidates(&submenu, candidates, action);
            }
            if item.get_action() == Some(action) {
                item.set_tag(0);
                candidates.push(item);
            }
        }
    }

    let mut candidates_for_removal = vec![];
    let main_menu = match Menu::get_main_menu() {
        Some(existing) => {
            mark_candidates(
                &existing,
                &mut candidates_for_removal,
                rio_perform_key_assignment_sel,
            );

            existing
        }
        None => {
            let menu = Menu::new_with_title("MainMenu");
            menu.assign_as_main_menu();
            menu
        }
    };

    let menu_titles = ["Rio", "Shell", "Edit", "View", "Window", "Help"];

    for title in menu_titles {
        let _submenu = main_menu.get_or_create_sub_menu(title, |menu| {
            if title == "Window" {
                menu.assign_as_windows_menu();
                // macOS will insert stuff at the top and bottom, so we add
                // a separator to tidy things up a bit
                menu.add_item(&MenuItem::new_separator());
            } else if title == "Rio" {
                menu.assign_as_app_menu();

                let rio_version = env!("CARGO_PKG_VERSION");
                let about_item = MenuItem::new_with(
                    &format!("Rio v{}", rio_version),
                    Some(rio_perform_key_assignment_sel),
                    "",
                );
                about_item.set_tool_tip("Click to copy version number");
                about_item.set_represented_item(RepresentedItem::KeyAssignment(
                    KeyAssignment::Copy(rio_version.to_string()),
                ));

                menu.add_item(&about_item);
                menu.add_item(&MenuItem::new_separator());
            } else if title == "Help" {
                menu.assign_as_help_menu();
            }
        });

        // let represented_item = RepresentedItem::KeyAssignment(cmd.action.clone());
        // let item = match submenu.get_item_with_represented_item(&represented_item) {
        //     Some(existing) => {
        //         existing.set_title(&cmd.brief);
        //         existing.set_key_equivalent(&short_cut);
        //         existing
        //     }
        //     None => {
        //         let item = MenuItem::new_with(
        //             &cmd.brief,
        //             Some(rio_perform_key_assignment_sel),
        //             &short_cut,
        //         );
        //         submenu.add_item(&item);
        //         item
        //     }
        // };
    }
}
