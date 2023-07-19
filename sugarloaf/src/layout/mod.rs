use crate::core::SugarloafStyle;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta<T: Default> {
    pub x: T,
    pub y: T,
}

#[derive(Default)]
pub struct SugarloafLayout {
    pub scale_factor: f32,
    pub line_height: f32,
    pub width: f32,
    pub height: f32,
    pub width_u32: u32,
    pub height_u32: u32,
    pub font_size: f32,
    pub original_font_size: f32,
    pub font_bound: f32,
    pub columns: usize,
    pub lines: usize,
    pub padding: Delta<f32>,
    pub style: SugarloafStyle,
    pub background_color: wgpu::Color,
    pub min_cols_lines: (usize, usize),
    pub sugarwidth: f32,
    pub sugarheight: f32,
}

#[inline]
fn update_styles(layout: &mut SugarloafLayout) {
    let text_scale = layout.font_size * layout.scale_factor;
    let new_styles = SugarloafStyle {
        line_height: layout.line_height,
        screen_position: (
            layout.padding.x * layout.scale_factor,
            layout.padding.y * layout.scale_factor,
        ),
        text_scale,
        icon_scale: text_scale / 1.5,
    };
    layout.style = new_styles;
}

// $ tput columns
// $ tput lines
#[inline]
fn compute(
    width: f32,
    height: f32,
    scale_factor: f32,
    font_size: f32,
    line_height: f32,
    font_bound: f32,
    padding: Delta<f32>,
    min_cols_lines: (usize, usize),
) -> (usize, usize) {
    let padding_x = ((padding.x) * scale_factor).floor();
    // let padding_y = ((padding.y) * scale_factor).floor();
    let padding_y = (padding.y * 2.).floor();

    let mut lines = (height / scale_factor) - padding_y;
    lines /= font_size * line_height;
    let visible_lines = std::cmp::max(lines as usize, min_cols_lines.1);

    let mut visible_columns = ((width) / scale_factor) - padding_x;
    visible_columns /= font_bound;
    let visible_columns = std::cmp::max(visible_columns as usize, min_cols_lines.0);

    (visible_columns, visible_lines)
}

impl SugarloafLayout {
    pub fn new(
        width: f32,
        height: f32,
        padding: (f32, f32),
        scale_factor: f32,
        font_size: f32,
        line_height: f32,
        min_cols_lines: (usize, usize),
    ) -> SugarloafLayout {
        let style = SugarloafStyle::default();

        // This is an estimation of the font_size however cannot be
        // relied entirely. We this value it until sugarloaf process the font bounds.
        let font_bound = font_size / 2.0;

        let mut layout = SugarloafLayout {
            width,
            width_u32: width as u32,
            height,
            height_u32: height as u32,
            columns: 80,
            lines: 25,
            scale_factor,
            original_font_size: font_size,
            font_size,
            sugarwidth: font_size,
            sugarheight: font_size,
            font_bound,
            line_height,
            style,
            padding: Delta {
                x: padding.0,
                y: padding.1,
            },
            background_color: wgpu::Color::BLACK,
            min_cols_lines,
        };

        update_styles(&mut layout);
        layout
    }

    pub fn rescale(&mut self, scale_factor: f32) -> &mut Self {
        self.scale_factor = scale_factor;
        self
    }

    pub fn resize(&mut self, width: u32, height: u32) -> &mut Self {
        self.width_u32 = width;
        self.height_u32 = height;
        self.width = width as f32;
        self.height = height as f32;
        self
    }

    pub fn increase_font_size(&mut self) -> bool {
        if self.font_size < 80.0 {
            self.font_size += 2.0;
            return true;
        }
        false
    }

    pub fn decrease_font_size(&mut self) -> bool {
        if self.font_size > 2.0 {
            self.font_size -= 2.0;
            return true;
        }
        false
    }

    pub fn reset_font_size(&mut self) -> bool {
        if self.font_size != self.original_font_size {
            self.font_size = self.original_font_size;
            return true;
        }
        false
    }

    pub fn update(&mut self) -> &mut Self {
        update_styles(self);
        let (columns, lines) = compute(
            self.width,
            self.height,
            self.scale_factor,
            self.font_size,
            self.line_height,
            self.font_bound,
            self.padding,
            self.min_cols_lines,
        );
        self.columns = columns;
        self.lines = lines;
        self
    }

    pub fn update_columns_lines_per_font_bound(&mut self, font_bound: f32) {
        self.font_bound = font_bound / self.scale_factor;

        // SugarStack is a primitive representation of columns data
        let current_stack_bound = self.font_bound * self.columns as f32;
        let expected_stack_bound = self.width / self.scale_factor - self.font_bound;

        log::info!("expected columns {}", self.columns);
        if current_stack_bound < expected_stack_bound {
            let stack_difference =
                ((expected_stack_bound - current_stack_bound) / self.font_bound) as usize;
            log::info!("recalculating columns due to font width, adding more {stack_difference:?} columns");
            self.columns += stack_difference;
        }

        if current_stack_bound > expected_stack_bound {
            let stack_difference =
                ((current_stack_bound - expected_stack_bound) / self.font_bound) as usize;
            log::info!("recalculating columns due to font width, removing {stack_difference:?} columns");
            self.columns -= stack_difference;
        }
    }

    // This method will run over the new font and font_size
    pub fn recalculate(
        &mut self,
        font_size: f32,
        line_height: f32,
        padding_x: f32,
    ) -> &mut Self {
        let mut should_apply_changes = false;
        if self.font_size != font_size {
            self.font_size = font_size;
            self.original_font_size = font_size;
            should_apply_changes = true;
        }

        if self.line_height != line_height {
            self.line_height = line_height;
            should_apply_changes = true;
        }

        if self.padding.x != padding_x {
            self.padding.x = padding_x;
            should_apply_changes = true;
        }

        if should_apply_changes {
            update_styles(self);
        }
        self
    }
}
