use miniquad::*;

struct Stage {
    ctx: GlContext,
}
impl EventHandler for Stage {
    fn update(&mut self) {}

    fn draw(&mut self) {
        self.ctx.clear(Some((0., 1., 0., 1.)), None, None);
    }
}

fn main() {
    miniquad::start(
        conf::Conf {
            window_title: "Miniquad".to_string(),
            window_width: 1024,
            window_height: 768,
            fullscreen: true,
            ..Default::default()
        },
        || {
            Box::new(Stage {
                ctx: GlContext::new(),
            })
        },
    );
}
