use crate::components::core::{image::Handle, shapes::Rectangle};
use crate::components::layer::{self, LayerBrush};
use crate::components::rect::{Rect, RectBrush};
use crate::components::rich_text::RichTextBrush;
use crate::components::text;
use crate::content::{Content, ContentBuilder};
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
use crate::layout::SpanStyle;
use crate::layout::SugarloafLayout;
use ab_glyph::{self, Font as GFont, FontArc, PxScale};
use core::fmt::{Debug, Formatter};
use fnv::FnvHashMap;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use unicode_width::UnicodeWidthChar;

#[cfg(target_arch = "wasm32")]
pub struct Database;

#[derive(Copy, Clone, PartialEq)]
pub struct CachedSugar {
    font_id: FontId,
    char_width: f32,
    px_scale: Option<PxScale>,
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
    pub content: ContentBuilder,
    rect_brush: RectBrush,
    layer_brush: LayerBrush,
    rich_text_brush: RichTextBrush,
    graphic_rects: FnvHashMap<crate::SugarGraphicId, GraphicRect>,
    rects: Vec<Rect>,
    fonts: SugarloafFonts,
    kind: SugarloafRendererFeatures,
    text_y: f32,
    current_row: u16,
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

#[derive(Copy, Clone)]
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

#[derive(Default)]
pub enum SugarloafRendererFeatures {
    Disabled,
    #[default]
    Enabled,
}

pub struct SugarloafRenderer {
    pub power_preference: wgpu::PowerPreference,
    pub backend: wgpu::Backends,
    pub features: SugarloafRendererFeatures,
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
            features: SugarloafRendererFeatures::default(),
        }
    }
}

impl SugarloafWindow {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        self.handle
    }

    fn raw_display_handle(&self) -> raw_window_handle::RawDisplayHandle {
        self.display
    }
}

impl HasWindowHandle for SugarloafWindow {
    fn window_handle(&self) -> std::result::Result<WindowHandle, HandleError> {
        let raw = self.raw_window_handle();
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for SugarloafWindow {
    fn display_handle(&self) -> Result<DisplayHandle, HandleError> {
        let raw = self.raw_display_handle();
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

unsafe impl Send for SugarloafWindow {}
unsafe impl Sync for SugarloafWindow {}

impl Sugarloaf {
    pub async fn new(
        window: SugarloafWindow,
        renderer: SugarloafRenderer,
        fonts: SugarloafFonts,
        layout: SugarloafLayout,
        #[allow(unused)] db: Option<&Database>,
    ) -> Result<Sugarloaf, SugarloafWithErrors> {
        let ctx = Context::new(window, &renderer).await;
        let mut sugarloaf_errors = None;

        #[cfg(not(target_arch = "wasm32"))]
        let loader = Font::load(fonts.to_owned(), db);
        #[cfg(target_arch = "wasm32")]
        let loader = Font::load(fonts.to_owned());

        let (loaded_fonts, fonts_not_found) = loader;

        if !fonts_not_found.is_empty() {
            sugarloaf_errors = Some(SugarloafErrors { fonts_not_found });
        }

        let text_brush = text::GlyphBrushBuilder::using_fonts(loaded_fonts)
            .build(&ctx.device, ctx.format);
        let rect_brush = RectBrush::init(&ctx);
        let layer_brush = LayerBrush::new(&ctx);
        let rich_text_brush = RichTextBrush::new(&ctx);

        let content = Content::builder();
        let instance = Sugarloaf {
            sugar_cache: FnvHashMap::default(),
            graphics: SugarloafGraphics::new(),
            content,
            layer_brush,
            fonts,
            ctx,
            rect_brush,
            rich_text_brush,
            rects: vec![],
            graphic_rects: FnvHashMap::default(),
            text_brush,
            layout,
            text_y: 0.0,
            current_row: 0,
            kind: renderer.features,
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

            let (loaded_fonts, fonts_not_found) = loader;
            if !fonts_not_found.is_empty() {
                return Some(SugarloafErrors { fonts_not_found });
            }

            // Clean font cache per instance
            self.sugar_cache = FnvHashMap::default();

            let text_brush = text::GlyphBrushBuilder::using_fonts(loaded_fonts)
                .build(&self.ctx.device, self.ctx.format);
            self.text_brush = text_brush;
            self.fonts = fonts;
        }

        None
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

        let mut px_scale = None;
        let char_width = sugar.content.width().unwrap_or(1) as f32;

        match font_id {
            // Icons will look for width 1
            FontId(FONT_ID_ICONS) => {
                px_scale = Some(PxScale {
                    x: self.layout.scaled_sugarwidth,
                    y: self.layout.scaled_sugarheight,
                });
            }

            FontId(FONT_ID_UNICODE) | FontId(FONT_ID_SYMBOL) => {
                // println!("FONT_ID_UNICODE {:?}", char_width);
                px_scale = Some(PxScale {
                    x: self.layout.scaled_sugarwidth * char_width,
                    y: self.layout.scaled_sugarheight,
                });
            }

            FontId(FONT_ID_EMOJIS) => {
                // scale_target = self.layout.sugarwidth * 2.0;
                px_scale = Some(PxScale {
                    x: self.layout.scaled_sugarwidth * 2.0,
                    y: self.layout.scaled_sugarheight,
                });
            }

            // FontId(FONT_ID_REGULAR) => {
            // px_scale = Some(PxScale {
            //     x: self.layout.scaled_sugarwidth * 2.0,
            //     y: self.layout.scaled_sugarheight,
            // })
            // }
            FontId(_) => {}
        }

        let cached_sugar = CachedSugar {
            font_id,
            char_width,
            px_scale,
        };

        self.sugar_cache.insert(
            sugar.content,
            CachedSugar {
                font_id,
                char_width,
                px_scale,
            },
        );

        cached_sugar
    }

    #[inline]
    pub fn stack(&mut self, mut stack: SugarStack) {
        match self.kind {
            SugarloafRendererFeatures::Disabled => {
                self.stack_text(&mut stack);
            }
            SugarloafRendererFeatures::Enabled => {
                self.stack_rich_text(&mut stack);
            }
        }
    }

    #[inline]
    fn stack_rich_text(&mut self, stack: &mut SugarStack) {
        self.content.enter_span(&[
            SpanStyle::family_list("Fira Code, Apple Symbols, Arial Unicode MS, Fira Code Nerd Font, georgia, serif"),
            SpanStyle::Size(self.layout.font_size),
            SpanStyle::LineSpacing(self.layout.line_height),
            SpanStyle::features(&[("dlig", 1).into(), ("hlig", 1).into()][..]),
        ]);
        let size = stack.len();
        let mut content = String::from("");
        for i in 0..size {
            if i < size - 1
                && stack[i].foreground_color == stack[i + 1].foreground_color
                && stack[i].background_color == stack[i + 1].background_color
                && stack[i].decoration.is_none()
                && stack[i + 1].decoration.is_none()
                && stack[i].media.is_none()
            {
                content.push_str(&stack[i].content.to_string());
            } else {
                if content.len() > 0 {
                    self.content.add_text(&content);
                    content = String::from("");
                }

                self.content.add_text(&stack[i].content.to_string());
            }
        }
        self.content.add_text("\n");
        self.content.leave_span();
    }

    #[inline]
    fn stack_text(&mut self, stack: &mut SugarStack) {
        let mut x = 0.;
        let mod_pos_y = self.layout.style.screen_position.1;
        let mod_text_y = self.layout.scaled_sugarheight;

        let sugar_x = self.layout.scaled_sugarwidth;
        let sugar_width = self.layout.sugarwidth * 2.;

        let mut rect_builder = RectBuilder::new(0);
        let mut text_builder = TextBuilder::new(FontId(FONT_ID_REGULAR));
        let mut repeated = RepeatedSugar::new(0);

        if self.text_y == 0.0 {
            self.text_y = self.layout.style.screen_position.1;
        }

        let size = stack.len();
        for i in 0..size {
            let mut add_pos_x = sugar_x;
            let mut sugar_char_width = 1.;
            let rect_pos_x = self.layout.style.screen_position.0 + x;

            let cached_sugar: CachedSugar = self.get_font_id(&mut stack[i]);
            // if stack[i].content != ' ' {
            //     println!("{:?} {:?} {:?}", stack[i].content, cached_sugar.char_width, cached_sugar.font_id);
            // }
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
                sugar_char_width = cached_sugar.char_width;
                add_pos_x += sugar_x;
            }

            let mut scale = PxScale {
                x: self.layout.scaled_sugarheight,
                y: self.layout.scaled_sugarheight,
            };
            if let Some(new_scale) = cached_sugar.px_scale {
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
                        scale: text_builder.scale,
                        font_id: text_builder.font_id,
                        extra: crate::components::text::Extra {
                            color: text_builder.color,
                            z: 0.0,
                        },
                    };

                    let section = crate::components::text::OwnedSection {
                        screen_position: (text_builder.pos_x, section_pos_y),
                        bounds: (self.layout.width, self.layout.height),
                        text: vec![text],
                        layout: crate::glyph::Layout::default_single_line()
                            .v_align(crate::glyph::VerticalAlign::Bottom)
                            .h_align(crate::glyph::HorizontalAlign::Left),
                    };

                    self.text_brush.queue(&section);
                    text_builder.reset();
                }

                let text = crate::components::text::OwnedText {
                    text: sugar_str,
                    scale,
                    font_id,
                    extra: crate::components::text::Extra {
                        color: stack[i].foreground_color,
                        z: 0.0,
                    },
                };

                let section = crate::components::text::OwnedSection {
                    screen_position: (section_pos_x, section_pos_y),
                    bounds: (self.layout.width, self.layout.height),
                    text: vec![text],
                    layout: crate::glyph::Layout::default_single_line()
                        .v_align(crate::glyph::VerticalAlign::Bottom)
                        .h_align(crate::glyph::HorizontalAlign::Left),
                };

                self.text_brush.queue(&section);
            } else {
                text_builder.add(
                    &sugar_str,
                    scale,
                    stack[i].foreground_color,
                    section_pos_x,
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

                let dec_pos_y = (scaled_rect_pos_y) + (decoration.relative_position.1);
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
                    self.layout.sugarheight * self.layout.line_height,
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
                    // println!("miss {:?}", sugar_media.id);
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
        self.text_y += self.layout.scaled_sugarheight * self.layout.line_height;
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
            scale: PxScale { x: scale, y: scale },
            font_id,
            extra: crate::components::text::Extra {
                color: [0., 0., 0., 0.],
                z: 0.0,
            },
        };

        let section = &crate::components::text::Section {
            screen_position: (0., 0.),
            bounds: (self.layout.width, self.layout.height),
            text: vec![text],
            layout: crate::glyph::Layout::default_single_line()
                .v_align(crate::glyph::VerticalAlign::Bottom)
                .h_align(crate::glyph::HorizontalAlign::Left),
        };

        self.text_brush.queue(section);

        if let Some(rect) = self.text_brush.glyph_bounds(section) {
            let width = rect.max.x - rect.min.x;
            let height = rect.max.y - rect.min.y;
            return (width, height);
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
        let font_bound = self.get_font_bounds(' ', FontId(FONT_ID_REGULAR), text_scale);

        self.layout.scaled_sugarwidth = font_bound.0;
        self.layout.scaled_sugarheight = font_bound.1;

        self.layout.sugarwidth = self.layout.scaled_sugarwidth / self.ctx.scale;
        self.layout.sugarheight = self.layout.scaled_sugarheight / self.ctx.scale;

        self.layout.update_columns_per_font_width();
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
                .v_align(crate::glyph::VerticalAlign::Bottom)
                .h_align(crate::glyph::HorizontalAlign::Left)
        } else {
            crate::glyph::Layout::default()
                .v_align(crate::glyph::VerticalAlign::Bottom)
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
    fn reset_state(&mut self) {
        match self.kind {
            SugarloafRendererFeatures::Disabled => {
                self.text_y = 0.0;
                self.rects = vec![];
                self.current_row = 0;
            }
            SugarloafRendererFeatures::Enabled => {
                self.content = Content::builder();
            }
        }

        self.graphic_rects = FnvHashMap::default();
    }

    #[inline]
    pub fn render(&mut self) {
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

                self.rect_brush
                    .render(&mut encoder, view, &self.rects, &mut self.ctx);

                let _ = self
                    .text_brush
                    .draw_queued(&mut self.ctx, &mut encoder, view);

                for entry_render in
                    &self.graphic_rects.keys().cloned().collect::<Vec<_>>()
                {
                    if let Some(entry) = self.graphic_rects.get(entry_render) {
                        if let Some(graphic_data) = self.graphics.get(&entry.id) {
                            let rows = entry.end_row - entry.start_row;
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

                self.rich_text_brush.render(
                    &self.content.build_ref(),
                    &mut self.ctx,
                    &mut encoder,
                    view,
                );

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

        self.reset_state();
    }
}
