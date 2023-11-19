use wa::*;

struct Stage {}

impl EventHandler for Stage {
    fn update(&mut self) {}

    fn draw(&mut self) {}

    fn char_event(&mut self, character: char, _: KeyMods, _: bool) {}
}

fn main() {
    wa::start(conf::Conf::default(), || Box::new(Stage {}));
}
