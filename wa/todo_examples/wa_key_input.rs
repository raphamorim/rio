#[cfg(target_os = "macos")]
use wa::*;

#[cfg(target_os = "macos")]
struct Stage {}

#[cfg(target_os = "macos")]
impl EventHandler for Stage {
    fn key_down_event(
        &mut self,
        keycode: KeyCode,
        _mods: ModifiersState,
        _repeat: bool,
        _character: Option<char>,
    ) {
        println!("{:?}", keycode);
    }

    // fn key_down(&mut self, character: char, _: KeyMods, _: bool) {
    //     match character {
    //         'z' => window::show_mouse(false),
    //         'x' => window::show_mouse(true),
    //         _ => (),
    //     }

    //     let icon = match character {
    //         '1' => CursorIcon::Default,
    //         '2' => CursorIcon::Help,
    //         '3' => CursorIcon::Pointer,
    //         '4' => CursorIcon::Wait,
    //         '5' => CursorIcon::Crosshair,
    //         '6' => CursorIcon::Text,
    //         '7' => CursorIcon::Move,
    //         '8' => CursorIcon::NotAllowed,
    //         '9' => CursorIcon::EWResize,
    //         '0' => CursorIcon::NSResize,
    //         'q' => CursorIcon::NESWResize,
    //         'w' => CursorIcon::NWSEResize,
    //         _ => return,
    //     };
    //     window::set_mouse_cursor(icon);
    // }
    fn process(&mut self) {
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    {
        App::start(|| false);
        conf::Conf::default(), || Box::new(Stage {}));

    }
}
