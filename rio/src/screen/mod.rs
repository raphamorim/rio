mod ansi;
mod messenger;
mod state;
pub mod window;

use crate::crosswords::grid::Scroll;
use crate::crosswords::Crosswords;
use crate::event::sync::FairMutex;
use crate::event::EventProxy;
use crate::layout::Layout;
use crate::performer::Machine;
use log::{info, warn};
use messenger::Messenger;
use state::State;
use std::borrow::Cow;
use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;
use sugarloaf::{RendererTarget, Sugarloaf};
use teletypewriter::create_pty;

pub struct Screen {
    sugarloaf: Sugarloaf,
    terminal: Arc<FairMutex<Crosswords<EventProxy>>>,
    pub messenger: Messenger,
    layout: Layout,
    state: State,
}

impl Screen {
    pub async fn new(
        winit_window: &winit::window::Window,
        config: &Rc<config::Config>,
        event_proxy: EventProxy,
    ) -> Result<Screen, Box<dyn Error>> {
        let shell = std::env::var("SHELL")?;
        let size = winit_window.inner_size();
        let scale = winit_window.scale_factor();

        let mut layout = Layout::new(
            size.width as f32,
            size.height as f32,
            scale as f32,
            config.style.font_size,
        );
        let (columns, rows) = layout.compute();
        let pty = create_pty(&Cow::Borrowed(&shell), columns as u16, rows as u16);

        let power_preference: wgpu::PowerPreference = match config.performance {
            config::Performance::High => wgpu::PowerPreference::HighPerformance,
            config::Performance::Low => wgpu::PowerPreference::LowPower,
        };

        let sugarloaf = Sugarloaf::new(
            RendererTarget::Desktop,
            winit_window,
            power_preference,
            config.style.font.to_string(),
        )
        .await?;

        let state = State::new(config);

        let event_proxy_clone = event_proxy.clone();
        let terminal: Arc<FairMutex<Crosswords<EventProxy>>> =
            Arc::new(FairMutex::new(Crosswords::new(columns, rows, event_proxy)));

        let machine = Machine::new(Arc::clone(&terminal), pty, event_proxy_clone)?;
        let channel = machine.channel();
        machine.spawn();
        let messenger = Messenger::new(channel);

        Ok(Screen {
            sugarloaf,
            terminal,
            layout,
            messenger,
            state,
        })
    }

    #[inline]
    pub fn propagate_modifiers_state(&mut self, state: winit::event::ModifiersState) {
        self.messenger.set_modifiers(state);
    }

    #[inline]
    pub fn input_keycode(
        &mut self,
        // _scancode: u32,
        virtual_keycode: Option<winit::event::VirtualKeyCode>,
    ) {
        info!("received keycode {:?}", virtual_keycode);

        if let Some(keycode) = virtual_keycode {
            let _ = self.messenger.send_keycode(keycode);
        } else {
            warn!("error keycode not as Some");
        }
    }

    #[inline]
    pub fn skeleton(&mut self, color: colors::ColorWGPU) {
        self.sugarloaf.init(color, self.layout.styles.term);
    }

    #[inline]
    pub fn render(&mut self) {
        let mut terminal = self.terminal.lock();
        let visible_rows = terminal.visible_rows();
        let cursor_position = terminal.cursor();
        drop(terminal);

        self.state.update(
            visible_rows,
            cursor_position,
            &mut self.sugarloaf,
            self.layout.styles.term,
        );

        self.sugarloaf.render();

        // self.sugarloaf.set_cursor(cursor_position, false);

        //     let mut line_height: f32 = 0.0;
        // let cursor_row = self.cursor.position.1;
        // for (i, row) in rows.iter().enumerate() {
        //     self.render_row(row, style, line_height, cursor_row == i);
        //     line_height += style.text_scale;
        // }

        // let mut row_text: Vec<OwnedText> = vec![];
        // let columns: usize = row.len();
        // for column in 0..columns {
        //     let square = &row.inner[column];
        //     let sugar = self.create_sugar(square);

        //     // self.sugarloaf.add(sugar);

        //     if has_cursor && column == self.cursor.position.0 {
        //         self.sugarloaf.sugar((self.cursor.content, self.named_colors.cursor, self.named_colors.cursor));
        //     } else {
        //         self.sugarloaf.sugar(self.create_sugar(square));
        //     }

        //     // Render last column and break row
        //     if column == (columns - 1) {
        //         let section = &OwnedSection {
        //             screen_position: (
        //                 style.screen_position.0,
        //                 style.screen_position.1 + line_height,
        //             ),
        //             bounds: style.bounds,
        //             text: row_text,
        //             layout: glyph_brush::Layout::default_single_line()
        //                 .v_align(glyph_brush::VerticalAlign::Bottom),
        //         };

        //         // println!("{:?}", self.brush.glyph_bounds(section));

        //         self.brush.queue(section);

        //         break;
        //     }
        // }

        // self.sugarloaf
        //     .set_cursor(cursor_position)
        //     .term(visible_rows, self.layout.styles.term)
        //     .render();
    }

    #[inline]
    pub fn scroll(&mut self, _new_scroll_x_px: f64, new_scroll_y_px: f64) {
        // let width = self.layout.width as f64;
        // let height = self.layout.height as f64;

        // if self
        //     .ctx
        //     .terminal()
        //     .mode()
        //     .contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
        //     && !self.ctx.modifiers().shift()
        // {
        // // let multiplier = f64::from(self.ctx.config().terminal_config.scrolling.multiplier);

        // // self.layout.mouse_mut().accumulated_scroll.x += new_scroll_x_px;//* multiplier;
        // // self.layout.mouse_mut().accumulated_scroll.y += new_scroll_y_px;// * multiplier;

        // // // The chars here are the same as for the respective arrow keys.
        // let line_cmd = if new_scroll_y_px > 0. { b'A' } else { b'B' };
        // let column_cmd = if new_scroll_x_px > 0. { b'D' } else { b'C' };

        // // let lines = (self.layout.cursor.accumulated_scroll.y / self.layout.font_size as f64).abs() as usize;
        // let lines = 1;
        // let columns = (self.layout.cursor.accumulated_scroll.x / width).abs() as usize;

        // let mut content = Vec::with_capacity(3 * (lines + columns));

        // for _ in 0..lines {
        //     content.push(0x1b);
        //     content.push(b'O');
        //     content.push(line_cmd);
        // }

        // for _ in 0..columns {
        //     content.push(0x1b);
        //     content.push(b'O');
        //     content.push(column_cmd);
        // }

        // println!("{:?} {:?} {:?} {:?}", content, lines, columns, self.layout.cursor);
        // if content.len() > 0 {
        //     self.messenger.write_to_pty(content);
        // }
        // }

        self.layout.mouse_mut().accumulated_scroll.y +=
            new_scroll_y_px * self.layout.mouse.multiplier;
        let lines = (self.layout.mouse.accumulated_scroll.y
            / self.layout.font_size as f64) as i32;

        if lines != 0 {
            let mut terminal = self.terminal.lock();
            terminal.scroll_display(Scroll::Delta(lines));
            drop(terminal);
        }
    }

    pub fn layout(&mut self) -> &mut Layout {
        &mut self.layout
    }

    #[inline]
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) -> &mut Self {
        self.sugarloaf.resize(new_size.width, new_size.height);
        self.layout
            .set_size(new_size.width, new_size.height)
            .update();
        let (c, l) = self.layout.compute();

        let mut terminal = self.terminal.lock();
        terminal.resize::<Layout>(self.layout.columns, self.layout.rows);
        drop(terminal);

        let _ = self.messenger.send_resize(
            new_size.width as u16,
            new_size.height as u16,
            c as u16,
            l as u16,
        );
        self
    }

    pub fn set_scale(
        &mut self,
        new_scale: f32,
        new_size: winit::dpi::PhysicalSize<u32>,
    ) -> &mut Self {
        self.sugarloaf
            .resize(new_size.width, new_size.height)
            .rescale(new_scale);

        self.layout
            .set_scale(new_scale)
            .set_size(new_size.width, new_size.height)
            .update();
        self
    }
}
