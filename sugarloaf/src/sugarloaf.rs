use crate::components::core::{image::Handle, shapes::Rectangle};
use crate::components::layer::{self, LayerBrush};
use crate::components::rect::{Rect, RectBrush};
use crate::components::text;
use crate::context::Context;
use crate::core::{
    ImageProperties, RectBuilder, RepeatedSugar, Sugar, SugarStack, TextBuilder,
};
use crate::font::fonts::{SugarloafFont, SugarloafFonts};
#[cfg(not(target_arch = "wasm32"))]
use crate::font::loader::Database;
use crate::font::Font;
use crate::font::{
    FONT_ID_BOLD, FONT_ID_BOLD_ITALIC, FONT_ID_EMOJIS, FONT_ID_ICONS, FONT_ID_ITALIC,
    FONT_ID_REGULAR, FONT_ID_SYMBOL, FONT_ID_UNICODE,
};
use crate::glyph::{FontId, GlyphCruncher};
use crate::graphics::SugarloafGraphics;
use crate::layout::SugarloafLayout;
use ab_glyph::{self, Font as GFont, FontArc, PxScale};
use core::fmt::{Debug, Formatter};
use fnv::FnvHashMap;
use unicode_width::UnicodeWidthChar;

#[cfg(target_arch = "wasm32")]
pub struct Database;

#[derive(Copy, Clone, PartialEq)]
pub struct CachedSugar {
    font_id: FontId,
    char_width: f32,
    monospaced_font_scale: Option<f32>,
}

struct GraphicRect {
    id: crate::graphics::SugarGraphicId,
    #[allow(unused)]
    height: u16,
    width: u16,
    pos_x: f32,
    pos_y: f32,
    columns: f32,
    start_row: f32,
    end_row: f32,
}

pub struct Sugarloaf {
    sugar_cache: FnvHashMap<char, CachedSugar>,
    pub ctx: Context,
    pub layout: SugarloafLayout,
    pub graphics: SugarloafGraphics,
    text_brush: text::GlyphBrush<()>,
    rect_brush: RectBrush,
    layer_brush: LayerBrush,
    graphic_rects: FnvHashMap<crate::SugarGraphicId, GraphicRect>,
    rects: Vec<Rect>,
    text_y: f32,
    current_row: u16,
    font_bound: (f32, f32),
    fonts: SugarloafFonts,
    is_text_monospaced: bool,
}

#[derive(Debug)]
pub struct SugarloafErrors {
    pub fonts_not_found: Vec<SugarloafFont>,
}

pub struct SugarloafWithErrors {
    pub instance: Sugarloaf,
    pub errors: SugarloafErrors,
}

impl Debug for SugarloafWithErrors {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.errors)
    }
}

pub struct SugarloafWindowSize {
    pub width: u32,
    pub height: u32,
}

pub struct SugarloafWindow {
    pub handle: raw_window_handle::RawWindowHandle,
    pub display: raw_window_handle::RawDisplayHandle,
    pub size: SugarloafWindowSize,
    pub scale: f32,
}

pub struct SugarloafRenderer {
    pub power_preference: wgpu::PowerPreference,
    pub backend: wgpu::Backends,
}

impl Default for SugarloafRenderer {
    fn default() -> SugarloafRenderer {
        #[cfg(target_arch = "wasm32")]
        let default_backend = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
        #[cfg(not(target_arch = "wasm32"))]
        let default_backend = wgpu::Backends::all();

        SugarloafRenderer {
            power_preference: wgpu::PowerPreference::HighPerformance,
            backend: default_backend,
        }
    }
}

unsafe impl raw_window_handle::HasRawWindowHandle for SugarloafWindow {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        self.handle
    }
}

unsafe impl raw_window_handle::HasRawDisplayHandle for SugarloafWindow {
    fn raw_display_handle(&self) -> raw_window_handle::RawDisplayHandle {
        self.display
    }
}

impl Sugarloaf {
    // pub fn empty() -> Sugarloaf {
    //     Sugarloaf {
    //         sugar_cache: HashMap::new(),
    //         graphics: SugarloafGraphics::new(),
    //         layer_brush,
    //         fonts,
    //         ctx,
    //         rect_brush,
    //         rects: vec![],
    //         graphic_rects: BTreeMap::new(),
    //         text_brush,
    //         text_y: 0.0,
    //         current_row: 0,
    //         font_bound: (0.0, 0.0),
    //         layout: SugarloafLayout::default(),
    //         is_text_monospaced: true,
    //     }
    // }

    pub async fn new(
        raw_window_handle: &SugarloafWindow,
        renderer: SugarloafRenderer,
        fonts: SugarloafFonts,
        layout: SugarloafLayout,
        #[allow(unused)] db: Option<&Database>,
    ) -> Result<Sugarloaf, SugarloafWithErrors> {
        let ctx = Context::new(raw_window_handle, &renderer).await;
        let mut sugarloaf_errors = None;

        #[cfg(not(target_arch = "wasm32"))]
        let loader = Font::load(fonts.to_owned(), db);
        #[cfg(target_arch = "wasm32")]
        let loader = Font::load(fonts.to_owned());

        let (is_text_monospaced, loaded_fonts, fonts_not_found) = loader;

        if !fonts_not_found.is_empty() {
            sugarloaf_errors = Some(SugarloafErrors { fonts_not_found });
        }

        let text_brush = text::GlyphBrushBuilder::using_fonts(loaded_fonts)
            .build(&ctx.device, ctx.format);
        let rect_brush = RectBrush::init(&ctx);
        let layer_brush = LayerBrush::new(&ctx);

        let instance = Sugarloaf {
            sugar_cache: FnvHashMap::default(),
            graphics: SugarloafGraphics::new(),
            layer_brush,
            fonts,
            ctx,
            rect_brush,
            rects: vec![],
            graphic_rects: FnvHashMap::default(),
            text_brush,
            text_y: 0.0,
            current_row: 0,
            font_bound: (0.0, 0.0),
            layout,
            is_text_monospaced,
        };

        if let Some(errors) = sugarloaf_errors {
            return Err(SugarloafWithErrors { instance, errors });
        }

        Ok(instance)
    }

    #[allow(unused)]
    pub fn clear(&mut self) {
        match self.ctx.surface.get_current_texture() {
            Ok(frame) => {
                let mut encoder = self.ctx.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: None },
                );

                let view = &frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    label: Some("sugarloaf::init -> Clear frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.layout.background_color),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                });
                self.ctx.queue.submit(Some(encoder.finish()));
                frame.present();
            }
            Err(error) => {
                if error == wgpu::SurfaceError::OutOfMemory {
                    panic!("Swapchain error: {error}. Rendering cannot continue.")
                }
            }
        }
    }

    #[inline]
    pub fn update_font(
        &mut self,
        fonts: SugarloafFonts,
        #[allow(unused)] db: Option<&Database>,
    ) -> Option<SugarloafErrors> {
        if self.fonts != fonts {
            log::info!("requested a font change");

            #[cfg(not(target_arch = "wasm32"))]
            let loader = Font::load(fonts.to_owned(), db);
            #[cfg(target_arch = "wasm32")]
            let loader = Font::load(fonts.to_owned());

            let (is_text_monospaced, loaded_fonts, fonts_not_found) = loader;
            if !fonts_not_found.is_empty() {
                return Some(SugarloafErrors { fonts_not_found });
            }

            // Clean font cache per instance
            self.sugar_cache = FnvHashMap::default();

            let text_brush = text::GlyphBrushBuilder::using_fonts(loaded_fonts)
                .build(&self.ctx.device, self.ctx.format);
            self.text_brush = text_brush;
            self.fonts = fonts;
            self.is_text_monospaced = is_text_monospaced;
        }

        None
    }

    #[inline]
    pub fn find_scale(
        &mut self,
        target_scale: f32,
        content: char,
        font_id: FontId,
    ) -> Option<f32> {
        let mut found = false;
        let mut scale = self.layout.style.text_scale;
        while !found && scale > 0.0 {
            let width = self.get_font_bounds(content, font_id, scale).0
                / self.layout.scale_factor;

            if width <= target_scale {
                found = true;
            } else {
                scale -= 1.0;
            }
        }

        log::info!("find_scale: {:?} {:?} {}", content, font_id, scale);
        Some(scale)
    }

    #[inline]
    pub fn get_font_id(&mut self, sugar: &mut Sugar) -> CachedSugar {
        if let Some(cached_sugar) = self.sugar_cache.get(&sugar.content) {
            return *cached_sugar;
        }

        #[allow(clippy::unnecessary_to_owned)]
        let fonts: &[FontArc] = &self.text_brush.fonts().to_owned();
        let mut font_id = FontId(FONT_ID_REGULAR);

        for (idx, _font_arc) in fonts.iter().enumerate() {
            let found_glyph_id = fonts[idx].glyph_id(sugar.content);
            if found_glyph_id != ab_glyph::GlyphId(0) {
                font_id = FontId(idx);
                break;
            }
        }

        let mut monospaced_font_scale = None;
        let char_width = sugar.content.width().unwrap_or(1) as f32;

        let mut scale_target: f32 = 0.;

        match font_id {
            // Icons will look for width 1
            FontId(FONT_ID_ICONS) => {
                scale_target = self.layout.sugarwidth;
            }

            FontId(FONT_ID_UNICODE) | FontId(FONT_ID_SYMBOL) => {
                scale_target = if char_width > 1. {
                    self.layout.sugarwidth * 2.0
                } else {
                    self.layout.sugarwidth
                };
            }

            FontId(FONT_ID_EMOJIS) => {
                scale_target = self.layout.sugarwidth * 2.0;
            }

            FontId(FONT_ID_REGULAR) => {
                if !self.is_text_monospaced {
                    log::warn!("aligning non monospaced font {}", sugar.content);
                    scale_target = if char_width > 1. {
                        self.layout.sugarwidth * 2.
                    } else {
                        self.layout.sugarwidth
                    };
                }
            }

            // Emojis does not need since it's loaded as monospaced
            // Text font only need for cases where it's not monospaced
            FontId(_) => {}
        }

        if scale_target != 0.0 {
            monospaced_font_scale = self.find_scale(scale_target, sugar.content, font_id);
        }

        let cached_sugar = CachedSugar {
            font_id,
            char_width,
            monospaced_font_scale,
        };

        self.sugar_cache.insert(
            sugar.content,
            CachedSugar {
                font_id,
                char_width,
                monospaced_font_scale,
            },
        );

        cached_sugar
    }

    #[inline]
    pub fn stack(&mut self, mut stack: SugarStack) {
        let mut x = 0.;
        let mod_pos_y = self.layout.style.screen_position.1;
        let mod_text_y = self.layout.scaled_sugarheight / 2.;

        let sugar_x = self.layout.scaled_sugarwidth;
        let sugar_width = self.layout.sugarwidth * 2.;

        let mut rect_builder = RectBuilder::new(0);
        let mut text_builder = TextBuilder::new(FontId(FONT_ID_REGULAR));
        let mut repeated = RepeatedSugar::new(0);

        let text_bound = self.layout.sugarheight * self.ctx.scale;
        if self.text_y == 0.0 {
            self.text_y = self.layout.style.screen_position.1;
        }

        let size = stack.len();
        for i in 0..size {
            let mut add_pos_x = sugar_x;
            let mut sugar_char_width = 1.;
            let rect_pos_x = self.layout.style.screen_position.0 + x;

            let cached_sugar: CachedSugar = self.get_font_id(&mut stack[i]);
            if i < size - 1
                && cached_sugar.char_width <= 1.
                && stack[i].content == stack[i + 1].content
                && stack[i].foreground_color == stack[i + 1].foreground_color
                && stack[i].background_color == stack[i + 1].background_color
                && stack[i].decoration.is_none()
                && stack[i + 1].decoration.is_none()
                && stack[i].media.is_none()
            {
                repeated.set(&stack[i], rect_pos_x, mod_text_y + self.text_y + mod_pos_y);
                x += add_pos_x;
                continue;
            }

            repeated.set_reset_on_next();

            let mut font_id = cached_sugar.font_id;
            let mut is_text_font = false;
            if cached_sugar.font_id == FontId(FONT_ID_REGULAR) {
                if let Some(style) = &stack[i].style {
                    if style.is_bold_italic {
                        font_id = FontId(FONT_ID_BOLD_ITALIC);
                    } else if style.is_bold {
                        font_id = FontId(FONT_ID_BOLD);
                    } else if style.is_italic {
                        font_id = FontId(FONT_ID_ITALIC);
                    }
                }
                is_text_font = true;
            }

            if cached_sugar.char_width > 1. {
                sugar_char_width += 1.;
                add_pos_x += sugar_x;
            }

            let mut scale = self.layout.style.text_scale;
            if let Some(new_scale) = cached_sugar.monospaced_font_scale {
                scale = new_scale;
            }

            let rect_pos_y = self.text_y + mod_pos_y;
            let width_bound = sugar_width * sugar_char_width;

            let quantity = if repeated.count() > 0 {
                1 + repeated.count()
            } else {
                1
            };

            let sugar_str: String = if quantity > 1 {
                repeated.content_str.to_owned()
            } else {
                stack[i].content.to_string()
            };

            let section_pos_x = if quantity > 1 {
                repeated.pos_x
            } else {
                rect_pos_x
            };

            let section_pos_y = if quantity > 1 {
                repeated.pos_y
            } else {
                mod_text_y + self.text_y + mod_pos_y
            };

            let is_last = i == size - 1;
            let has_different_color_font = text_builder.has_initialized
                && (text_builder.font_id != font_id
                    || text_builder.color != stack[i].foreground_color);

            // If the font_id is different from TextBuilder, OR is the last item of the stack,
            // OR does the text builder color is different than current sugar needs to wrap up
            // the text builder and also queue the current stack item.
            //
            // TODO: Accept diferent colors
            if !is_text_font || is_last || has_different_color_font {
                if text_builder.has_initialized {
                    let text = crate::components::text::OwnedText {
                        text: text_builder.content.to_owned(),
                        scale: PxScale::from(text_builder.scale),
                        font_id: text_builder.font_id,
                        extra: crate::components::text::Extra {
                            color: text_builder.color,
                            z: 0.0,
                        },
                    };

                    let section = crate::components::text::OwnedSection {
                        screen_position: (text_builder.pos_x, section_pos_y),
                        bounds: (text_builder.width_bound, text_bound),
                        text: vec![text],
                        layout: crate::glyph::Layout::default_single_line()
                            .v_align(crate::glyph::VerticalAlign::Center)
                            .h_align(crate::glyph::HorizontalAlign::Left),
                    };

                    self.text_brush.queue(&section);
                    text_builder.reset();
                }

                let text = crate::components::text::OwnedText {
                    text: sugar_str,
                    scale: PxScale::from(scale),
                    font_id,
                    extra: crate::components::text::Extra {
                        color: stack[i].foreground_color,
                        z: 0.0,
                    },
                };

                let section = crate::components::text::OwnedSection {
                    screen_position: (section_pos_x, section_pos_y),
                    bounds: (width_bound * quantity as f32, text_bound),
                    text: vec![text],
                    layout: crate::glyph::Layout::default_single_line()
                        .v_align(crate::glyph::VerticalAlign::Center)
                        .h_align(crate::glyph::HorizontalAlign::Left),
                };

                self.text_brush.queue(&section);
            } else {
                text_builder.add(
                    &sugar_str,
                    scale,
                    stack[i].foreground_color,
                    section_pos_x,
                    width_bound * quantity as f32,
                    font_id,
                );
            }

            let scaled_rect_pos_x = section_pos_x / self.ctx.scale;
            let scaled_rect_pos_y = rect_pos_y / self.ctx.scale;

            // The decoration cannot be added before the rect otherwise can lead
            // to issues in the renderer, therefore we need check if decoration does exists
            if let Some(decoration) = &stack[i].decoration {
                if rect_builder.quantity >= 1 {
                    self.rects.push(rect_builder.build());
                }

                self.rects.push(Rect {
                    position: [scaled_rect_pos_x, scaled_rect_pos_y],
                    color: stack[i].background_color,
                    size: [width_bound * quantity as f32, self.layout.sugarheight],
                });

                let dec_pos_y = (scaled_rect_pos_y)
                    + (decoration.relative_position.1 * self.layout.line_height);
                self.rects.push(Rect {
                    position: [
                        (scaled_rect_pos_x
                            + (add_pos_x * decoration.relative_position.0)
                                / self.ctx.scale),
                        dec_pos_y,
                    ],
                    color: decoration.color,
                    size: [
                        (width_bound * decoration.size.0),
                        (self.layout.sugarheight) * decoration.size.1,
                    ],
                });
            } else {
                rect_builder.add(
                    scaled_rect_pos_x,
                    scaled_rect_pos_y,
                    stack[i].background_color,
                    width_bound * quantity as f32,
                    self.layout.sugarheight,
                );

                // If the next rect background color is different the push rect
                if is_last || rect_builder.color != stack[i + 1].background_color {
                    self.rects.push(rect_builder.build());
                }
            }

            if let Some(sugar_media) = &stack[i].media {
                if let Some(rect) = self.graphic_rects.get_mut(&sugar_media.id) {
                    rect.columns += 1.0;
                    rect.end_row = self.current_row.into();
                } else {
                    println!("miss {:?}", sugar_media.id);
                    self.graphic_rects.insert(
                        sugar_media.id,
                        GraphicRect {
                            id: sugar_media.id,
                            height: sugar_media.height,
                            width: sugar_media.width,
                            pos_x: scaled_rect_pos_x,
                            pos_y: scaled_rect_pos_y,
                            columns: 1.0,
                            start_row: 1.0,
                            end_row: 1.0,
                        },
                    );
                }
            }

            if repeated.reset_on_next() {
                repeated.reset();
            }

            x += add_pos_x;
        }

        self.current_row += 1;
        self.text_y += self.layout.scaled_sugarheight;
    }

    #[inline]
    pub fn get_context(&self) -> &Context {
        &self.ctx
    }

    #[inline]
    pub fn get_scale(&self) -> f32 {
        self.ctx.scale
    }

    #[inline]
    pub fn get_font_bounds(
        &mut self,
        content: char,
        font_id: FontId,
        scale: f32,
    ) -> (f32, f32) {
        let text = crate::components::text::Text {
            text: &content.to_owned().to_string(),
            scale: PxScale::from(scale),
            font_id,
            extra: crate::components::text::Extra {
                color: [0., 0., 0., 0.],
                z: 0.0,
            },
        };

        let section = &crate::components::text::Section {
            screen_position: (0., 0.),
            bounds: (scale, scale),
            text: vec![text],
            layout: crate::glyph::Layout::default_single_line()
                .v_align(crate::glyph::VerticalAlign::Center)
                .h_align(crate::glyph::HorizontalAlign::Left),
        };

        self.text_brush.queue(section);

        if let Some(rect) = self.text_brush.glyph_bounds(section) {
            let width = rect.max.x - rect.min.x;
            let height = rect.max.y - rect.min.y;
            return (width, height * self.layout.line_height);
        }

        (0., 0.)
    }

    #[inline]
    pub fn set_background_color(&mut self, color: wgpu::Color) -> &mut Self {
        self.layout.background_color = color;
        self
    }

    #[inline]
    pub fn set_background_image(&mut self, image: &ImageProperties) -> &mut Self {
        let handle = Handle::from_path(image.path.to_owned());
        self.layout.background_image = Some(layer::types::Image::Raster {
            handle,
            bounds: Rectangle {
                width: image.width,
                height: image.height,
                x: image.x,
                y: image.y,
            },
        });
        self
    }

    /// calculate_bounds is a fake render operation that defines font bounds
    /// is an important function to figure out the cursor dimensions and background color
    /// but should be used as minimal as possible.
    ///
    /// For example: It is used in Rio terminal only in the initialization and
    /// configuration updates that leads to layout recalculation.
    ///
    #[inline]
    pub fn calculate_bounds(&mut self) {
        // Every time a font size change the cached bounds also changes
        self.sugar_cache = FnvHashMap::default();

        let text_scale = self.layout.style.text_scale;
        // Bounds are defined in runtime
        if self.is_text_monospaced {
            self.font_bound =
                self.get_font_bounds(' ', FontId(FONT_ID_REGULAR), text_scale);
        } else {
            self.font_bound =
                self.get_font_bounds('-', FontId(FONT_ID_REGULAR), text_scale);
        }

        self.layout.scaled_sugarwidth = self.font_bound.0;
        self.layout.scaled_sugarheight = self.font_bound.1;

        self.layout.sugarwidth = self.layout.scaled_sugarwidth / self.ctx.scale;
        self.layout.sugarheight = self.layout.scaled_sugarheight / self.ctx.scale;

        self.layout.update_columns_per_font_width();
    }

    #[inline]
    fn reset_state(&mut self) {
        self.text_y = 0.0;
    }

    #[inline]
    pub fn pile_rects(&mut self, mut instances: Vec<Rect>) -> &mut Self {
        self.rects.append(&mut instances);
        self
    }

    #[inline]
    pub fn text(
        &mut self,
        pos: (f32, f32),
        text_str: String,
        font_id_usize: usize,
        scale: f32,
        color: [f32; 4],
        single_line: bool,
    ) -> &mut Self {
        let font_id = FontId(font_id_usize);

        let text = crate::components::text::Text {
            text: &text_str,
            scale: PxScale::from(scale * self.ctx.scale),
            font_id,
            extra: crate::components::text::Extra { color, z: 0.0 },
        };

        let layout = if single_line {
            crate::glyph::Layout::default_single_line()
                .v_align(crate::glyph::VerticalAlign::Center)
                .h_align(crate::glyph::HorizontalAlign::Left)
        } else {
            crate::glyph::Layout::default()
                .v_align(crate::glyph::VerticalAlign::Center)
                .h_align(crate::glyph::HorizontalAlign::Left)
        };

        let section = &crate::components::text::Section {
            screen_position: (pos.0 * self.ctx.scale, pos.1 * self.ctx.scale),
            bounds: (self.layout.width, self.layout.height),
            text: vec![text],
            layout,
        };

        self.text_brush.queue(section);
        self
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) {
        self.ctx.resize(width, height);
        self.layout.resize(width, height).update();
    }

    #[inline]
    pub fn rescale(&mut self, scale: f32) {
        self.ctx.scale = scale;
        self.layout.rescale(scale).update();
    }

    #[inline]
    pub fn render(&mut self) {
        self.reset_state();

        match self.ctx.surface.get_current_texture() {
            Ok(frame) => {
                let mut encoder = self.ctx.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: None },
                );

                let view = &frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    label: Some("sugarloaf::render -> Clear frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.layout.background_color),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                if let Some(bg_image) = &self.layout.background_image {
                    self.layer_brush.prepare_ref(
                        &mut encoder,
                        &mut self.ctx,
                        &[bg_image],
                    );

                    self.layer_brush
                        .render_with_encoder(0, view, &mut encoder, None);
                }

                for entry_render in
                    &self.graphic_rects.keys().cloned().collect::<Vec<_>>()
                {
                    if let Some(entry) = self.graphic_rects.get(entry_render) {
                        if let Some(graphic_data) = self.graphics.get(&entry.id) {
                            let rows = entry.end_row - entry.start_row;
                            println!("{:?}", entry.columns);
                            println!("{:?}", rows);
                            println!("{:?}", self.current_row);
                            println!("{:?}", (rows) * self.layout.scaled_sugarheight);

                            let height = (rows - 2.) * self.layout.scaled_sugarheight;

                            let a = layer::types::Image::Raster {
                                handle: graphic_data.handle.clone(),
                                bounds: Rectangle {
                                    x: entry.pos_x,
                                    y: entry.pos_y,
                                    width: entry.width as f32,
                                    height,
                                },
                            };

                            self.layer_brush.prepare_ref(
                                &mut encoder,
                                &mut self.ctx,
                                &[&a],
                            );

                            self.layer_brush.render_with_encoder(
                                0,
                                view,
                                &mut encoder,
                                None,
                            );
                        }
                    }
                }

                self.rect_brush.render(
                    &mut encoder,
                    view,
                    (self.ctx.size.width, self.ctx.size.height),
                    &self.rects,
                    &mut self.ctx,
                );

                self.rects = vec![];
                self.graphic_rects = FnvHashMap::default();
                self.current_row = 0;

                let _ = self
                    .text_brush
                    .draw_queued(&mut self.ctx, &mut encoder, view);

                self.layer_brush.end_frame();

                self.ctx.queue.submit(Some(encoder.finish()));
                frame.present();
            }
            Err(error) => {
                if error == wgpu::SurfaceError::OutOfMemory {
                    panic!("Swapchain error: {error}. Rendering cannot continue.")
                }
            }
        }
    }
}
