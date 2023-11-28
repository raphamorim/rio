#[cfg(target_os = "macos")]
use wa::*;

#[cfg(target_os = "macos")]
struct Stage {}

#[cfg(target_os = "macos")]
impl EventHandler for Stage {
    fn update(&mut self) {}

    fn draw(&mut self) {}

    fn char_event(&mut self, character: char, _: KeyMods, _: bool) {
        println!("{:?}", character);
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    wa::start(conf::Conf::default(), || Box::new(Stage {}));
}
