#![allow(dead_code, unused_variables, unused_imports, unused_mut)]

mod comp;
mod doc;
mod wgpu_renderer;
mod layout;
mod util;

use comp::*;
use comp::{color, color::Color};
use layout::*;
use std::time::Instant;

fn main() {
    use clipboard2::Clipboard;
    let clipboard = clipboard2::SystemClipboard::new().unwrap();
    let el = EventLoop::new();
    let wb = WindowBuilder::new()
        .with_title("swash wgpu demo")
        .with_inner_size(LogicalSize::new(1024, 768))
        .with_resizable(true);
    let windowed_context = ContextBuilder::new()
        .with_vsync(false)
        .build_windowed(wb, &el)
        .unwrap();
    let windowed_context = unsafe { windowed_context.make_current().unwrap() };
    println!(
        "Pixel format of the window's GL context: {:?}",
        windowed_context.get_pixel_format()
    );
    gfx::gl::load(&windowed_context.context());
    const MARGIN: f32 = 12.;
    let mut keymods = glutin::event::ModifiersState::default();
    let mut dpi = windowed_context.window().scale_factor() as f32;
    let mut margin = MARGIN * dpi;
    let fonts = layout::FontLibrary::default();
    let mut lcx = LayoutContext::new(&fonts);
    let initial_size = windowed_context.window().inner_size();
    let mut layout = Paragraph::new();
    let mut doc = build_document();
    let mut first_run = true;
    let mut selection = Selection::default();
    let mut selection_rects: Vec<[f32; 4]> = Vec::new();
    let mut selecting = false;
    let mut selection_changed = false;
    let mut extend_to = ExtendTo::Point;
    let mut inserted = None;
    let mut last_time = Instant::now();
    let mut frame_count = 0;
    let mut total_time = 0f32;
    let mut title = String::from("");
    let mut mx = 0.;
    let mut my = 0.;
    let mut clicks = 0;
    let mut click_time = Instant::now();
    let mut cursor_on = true;
    let mut cursor_time = 0.;
    let mut needs_update = true;
    let mut size_changed = true;
    let mut dark_mode = false;
    let mut device = gfx::Device::new();
    let mut comp = comp::Compositor::new(2048);
    let mut dlist = comp::DisplayList::new();
    let mut align = Alignment::Start;
    let mut always_update = false;
    windowed_context
        .window()
        .set_cursor_icon(glutin::window::CursorIcon::Text);
    // let quad = gfx::FullscreenQuad::new();
    el.run(move |event, _, control_flow| {
        //println!("{:?}", event);
        *control_flow = ControlFlow::Poll;
        match event {
            Event::LoopDestroyed => return,
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::Resized(physical_size) => {
                    windowed_context.resize(physical_size);
                    selection_changed = true;
                    size_changed = true;
                }
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    dpi = scale_factor as f32;
                    margin = MARGIN * dpi;
                    needs_update = true;
                    selection_changed = true;
                    windowed_context.window().request_redraw();
                }
                WindowEvent::ModifiersChanged(mods) => keymods = mods,
                WindowEvent::CursorMoved { position, .. } => {
                    mx = position.x as f32;
                    my = position.y as f32;
                    if selecting {
                        selection =
                            selection.extend_to(&layout, mx - margin, my - margin, extend_to);
                        selection_changed = true;
                        cursor_time = 0.;
                        cursor_on = true;
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    use glutin::event::{ElementState, MouseButton};
                    if button != MouseButton::Left {
                        return;
                    }
                    cursor_time = 0.;
                    cursor_on = true;
                    if state == ElementState::Pressed {
                        let now = Instant::now();
                        if now.duration_since(click_time).as_secs_f32() < 0.25 {
                            if clicks == 3 {
                                clicks = 0;
                            }
                            clicks += 1;
                        } else {
                            clicks = 1;
                        }
                        click_time = now;
                        let x = mx - margin;
                        let y = my - margin;
                        selection = if clicks == 2 {
                            extend_to = ExtendTo::Word;
                            Selection::word_from_point(&layout, x, y)
                        } else if clicks == 3 {
                            extend_to = ExtendTo::Line;
                            Selection::line_from_point(&layout, x, y)
                        } else {
                            extend_to = ExtendTo::Point;
                            Selection::from_point(&layout, x, y)
                        };
                        selecting = true;
                        selection_changed = true;
                    } else {
                        selecting = false;
                    }
                }
                WindowEvent::ReceivedCharacter(ch) => {
                    // println!("got char {:?} [{}]", ch, ch as u32);
                    match ch as u32 {
                        8 | 13 | 127 => return,
                        _ => {}
                    }
                    if keymods.intersects(ModifiersState::CTRL | ModifiersState::LOGO) {
                        return;
                    }
                    if !selection.is_collapsed() {
                        if let Some(erase) = selection.erase(&layout) {
                            if let Some(offset) = doc.erase(erase) {
                                inserted = Some(offset);
                                if let Some(offs) = doc.insert(offset, ch) {
                                    inserted = Some(offs);
                                }
                                needs_update = true;
                            }
                        }
                    } else {
                        let place = selection.offset(&layout);
                        if let Some(offs) = doc.insert(place, ch) {
                            inserted = Some(offs);
                        }
                        needs_update = true;
                    }
                }
                WindowEvent::KeyboardInput { input, .. } => {
                    // use glutin::event::{ElementState, VirtualKeyCode, VirtualKeyCode::*};
                    // if input.state != ElementState::Pressed {
                    //     return;
                    // }
                    // if let Some(key) = input.virtual_keycode {
                    //     let shift = keymods.intersects(ModifiersState::SHIFT);
                    //     let ctrl = keymods.intersects(ModifiersState::CTRL);
                    //     let cmd = keymods.intersects(ModifiersState::LOGO);
                    //     windowed_context.window().request_redraw();
                    //     cursor_time = 0.;
                    //     cursor_on = true;
                    //     match key {
                    //         Return => {
                    //             let ch = '\n';
                    //             if !selection.is_collapsed() {
                    //                 if let Some(erase) = selection.erase(&layout) {
                    //                     if let Some(offset) = doc.erase(erase) {
                    //                         inserted = Some(offset);
                    //                         if let Some(offs) = doc.insert(offset, ch) {
                    //                             inserted = Some(offs);
                    //                         }
                    //                         needs_update = true;
                    //                     }
                    //                 }
                    //             } else {
                    //                 let place = selection.offset(&layout);
                    //                 if let Some(offs) = doc.insert(place, ch) {
                    //                     inserted = Some(offs);
                    //                 }
                    //                 needs_update = true;
                    //             }
                    //         }
                    //         Back => {
                    //             if let Some(erase) = selection.erase_previous(&layout) {
                    //                 if let Some(offset) = doc.erase(erase) {
                    //                     inserted = Some(offset);
                    //                     needs_update = true;
                    //                 }
                    //             }
                    //         }
                    //         Delete => {
                    //             if let Some(erase) = selection.erase(&layout) {
                    //                 if let Some(offset) = doc.erase(erase) {
                    //                     inserted = Some(offset);
                    //                     needs_update = true;
                    //                 }
                    //             }
                    //         }
                    //         C => {
                    //             if ctrl || cmd {
                    //                 let text =
                    //                     doc.get_selection(selection.normalized_range(&layout));
                    //                 clipboard.set_string_contents(text).ok();
                    //             }
                    //         }
                    //         V => {
                    //             if ctrl || cmd {
                    //                 if let Ok(text) = clipboard.get_string_contents() {
                    //                     if !selection.is_collapsed() {
                    //                         if let Some(erase) = selection.erase(&layout) {
                    //                             if let Some(offset) = doc.erase(erase) {
                    //                                 inserted = Some(offset);
                    //                                 if let Some(offs) =
                    //                                     doc.insert_str(offset, &text)
                    //                                 {
                    //                                     inserted = Some(offs);
                    //                                 }
                    //                                 needs_update = true;
                    //                             }
                    //                         }
                    //                     } else {
                    //                         let place = selection.offset(&layout);
                    //                         if let Some(offs) = doc.insert_str(place, &text) {
                    //                             inserted = Some(offs);
                    //                         }
                    //                         needs_update = true;
                    //                     }
                    //                 }
                    //             }
                    //         }
                    //         X => {
                    //             if ctrl || cmd {
                    //                 if !selection.is_collapsed() {
                    //                     let text =
                    //                     doc.get_selection(selection.normalized_range(&layout));
                    //                 clipboard.set_string_contents(text).ok();
                    //                     if let Some(erase) = selection.erase(&layout) {
                    //                         if let Some(offset) = doc.erase(erase) {
                    //                             inserted = Some(offset);
                    //                             needs_update = true;
                    //                         }
                    //                     }
                    //                 }
                    //             }
                    //         }
                    //         F1 => dark_mode = !dark_mode,
                    //         F2 => {
                    //             align = Alignment::Start;
                    //             size_changed = true;
                    //         }
                    //         F3 => {
                    //             align = Alignment::Middle;
                    //             size_changed = true;
                    //         }
                    //         F4 => {
                    //             align = Alignment::End;
                    //             size_changed = true;
                    //         }
                    //         F5 => {
                    //             //always_update = !always_update;
                    //         }
                    //         F7 => {
                    //             let mut clusters = Vec::new();
                    //             let mut u = 0;
                    //             for line in layout.lines() {
                    //                 for run in line.runs() {
                    //                     for (i, cluster) in run.visual_clusters().enumerate() {
                    //                         clusters.push((cluster, u, line.baseline()));
                    //                         u += 1;
                    //                     }
                    //                 }
                    //             }
                    //             let mut clusters2 = clusters.clone();
                    //             clusters2.sort_by(|a, b| a.0.offset().cmp(&b.0.offset()));
                    //             for (i, c2) in clusters2.iter().enumerate() {
                    //                 clusters[c2.1].1 = i;
                    //             }
                    //             let mut glyphs = Vec::new();
                    //             let mut x = 0.;
                    //             for cluster in &clusters {
                    //                 for mut glyph in cluster.0.glyphs() {
                    //                     glyph.x += x;
                    //                     glyph.y = cluster.2;
                    //                     x += glyph.advance;
                    //                     glyphs.push((cluster.1, glyph));
                    //                 }
                    //             }
                    //             let chars = doc.text.char_indices().collect::<Vec<_>>();
                    //             for (i, g) in glyphs.iter().enumerate() {
                    //                 if let Some((j, ch)) = chars.get(g.0).copied() {
                    //                     println!(
                    //                         "| {} | {} | {} | {} | {:.2}, {:.2} |",
                    //                         g.0, j, ch, g.1.id, g.1.x, g.1.y
                    //                     );
                    //                 }
                    //             }
                    //         }                            
                    //         Left => {
                    //             selection = if cmd {
                    //                 selection.home(&layout, shift)
                    //             } else {
                    //                 selection.previous(&layout, shift)
                    //             };
                    //             selection_changed = true;
                    //         }
                    //         Right => {
                    //             selection = if cmd {
                    //                 selection.end(&layout, shift)
                    //             } else {
                    //                 selection.next(&layout, shift)
                    //             };
                    //             selection_changed = true;
                    //         }
                    //         Home => {
                    //             selection = selection.home(&layout, shift);
                    //             selection_changed = true;
                    //         }
                    //         End => {
                    //             selection = selection.end(&layout, shift);
                    //             selection_changed = true;
                    //         }
                    //         Up => {
                    //             selection = selection.previous_line(&layout, shift);
                    //             selection_changed = true;
                    //         }
                    //         Down => {
                    //             selection = selection.next_line(&layout, shift);
                    //             selection_changed = true;
                    //         }
                    //         _ => {}
                    //     }
                    // }
                }
                _ => (),
            },
            Event::MainEventsCleared => {
                windowed_context.window().request_redraw();
            }
            Event::RedrawRequested(_) => {
                // let start = std::time::Instant::now();
                let cur_time = Instant::now();
                let dt = cur_time.duration_since(last_time).as_secs_f32();
                last_time = cur_time;
                frame_count += 1;
                total_time += dt;
                if total_time >= 1. {
                    use std::fmt::Write;
                    title.clear();
                    write!(
                        title,
                        "swash demo ({} fps)",
                        frame_count as f32 / total_time
                    )
                    .ok();
                    windowed_context.window().set_title(&title);
                    total_time = 0.;
                    frame_count = 0;
                }
                cursor_time += dt;
                if cursor_on {
                    if cursor_time > 0.5 {
                        cursor_time = 0.;
                        cursor_on = false;
                    }
                } else {
                    if cursor_time > 0.5 {
                        cursor_time = 0.;
                        cursor_on = true;
                    }
                }
                if first_run {
                    needs_update = true;
                }
                let window_size = windowed_context.window().inner_size();
                let w = window_size.width;
                let h = window_size.height;
                if always_update {
                    needs_update = true;
                }
                if needs_update {
                    let mut lb = lcx.builder(Direction::LeftToRight, None, dpi);
                    doc.layout(&mut lb);
                    layout.clear();
                    lb.build_into(&mut layout);
                    if first_run {
                        selection = Selection::from_point(&layout, 0., 0.);
                    }
                    first_run = false;
                    //layout.build_new_clusters();
                    needs_update = false;
                    size_changed = true;
                    selection_changed = true;
                }
                if size_changed {
                    let lw = w as f32 - margin * 2.;
                    layout.break_lines().break_remaining(lw, align);
                    size_changed = false;
                    selection_changed = true;
                }
                if let Some(offs) = inserted {
                    selection = Selection::from_offset(&layout, offs);
                }
                inserted = None;

                if selection_changed {
                    selection_rects.clear();
                    selection.regions_with(&layout, |r| {
                        selection_rects.push(r);
                    });
                    selection_changed = false;
                }

                let (fg, bg) = if dark_mode {
                    (color::WHITE_SMOKE, Color::new(20, 20, 20, 255))
                } else {
                    (color::BLACK, color::WHITE)
                };

                comp.begin();
                draw_layout(&mut comp, &layout, margin, margin, 512., fg);

                for r in &selection_rects {
                    let rect = [r[0] + margin, r[1] + margin, r[2], r[3]];
                    if dark_mode {
                        comp.draw_rect(rect, 600., Color::new(38, 79, 120, 255));
                    } else {
                        comp.draw_rect(rect, 600., Color::new(179, 215, 255, 255));
                    }
                }

                let (pt, ch, rtl) = selection.cursor(&layout);
                if ch != 0. && cursor_on {
                    let rect = [pt[0].round() + margin, pt[1].round() + margin, 1. * dpi, ch];
                    comp.draw_rect(rect, 0.1, fg);
                }
                dlist.clear();
                device.finish_composition(&mut comp, &mut dlist);

                unsafe {
                    gl::Viewport(0, 0, w as i32, h as i32);
                    let cc = bg.to_rgba_f32();
                    gl::ClearColor(cc[0], cc[1], cc[2], 1.0);
                    gl::ClearDepth(1.0);
                    gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
                    gl::Enable(gl::DEPTH_TEST);
                    gl::DepthFunc(gl::LESS);
                    gl::DepthMask(1);
                    device.render(w, h, &dlist);
                    gl::Flush();
                }
                windowed_context.swap_buffers().unwrap();
                // let duration = start.elapsed();
                // println!("Time elapsed in render() is: {:?}", duration);
            }
            _ => (),
        }
    });
}

fn draw_layout(
    comp: &mut comp::Compositor,
    layout: &Paragraph,
    x: f32,
    y: f32,
    depth: f32,
    color: Color,
) {
    let mut glyphs = Vec::new();
    for line in layout.lines() {
        let mut px = x + line.offset();
        for run in line.runs() {
            use comp::text::*;
            use comp::*;
            let font = run.font();
            let py = line.baseline() + y;
            let run_x = px;
            glyphs.clear();
            for cluster in run.visual_clusters() {
                for glyph in cluster.glyphs() {
                    let x = px + glyph.x;
                    let y = py - glyph.y;
                    px += glyph.advance;
                    glyphs.push(Glyph { id: glyph.id, x, y });
                }
            }
            let style = TextRunStyle {
                font: font.as_ref(),
                font_coords: run.normalized_coords(),
                font_size: run.font_size(),
                color,
                baseline: py,
                advance: px - run_x,
                underline: if run.underline() {
                    Some(UnderlineStyle {
                        offset: run.underline_offset(),
                        size: run.underline_size(),
                        color,
                    })
                } else {
                    None
                },
            };
            comp.draw_glyphs(
                Rect::new(run_x, py, style.advance, 1.),
                depth,
                &style,
                glyphs.iter(),
            );
        }
    }
}

fn build_document() -> doc::Document {
    use layout::*;
    let mut db = doc::Document::builder();

    use SpanStyle as S;

    let underline = &[
        S::Underline(true),
        S::UnderlineOffset(Some(-1.)),
        S::UnderlineSize(Some(1.)),
    ];

    db.enter_span(&[
        S::family_list("fira code, times, georgia, serif"),
        S::Size(18.),
        S::features(&[("dlig", 1).into(), ("hlig", 1).into()][..]),
    ]);
    db.enter_span(&[S::LineSpacing(1.2)]);
    db.enter_span(&[S::family_list("baskerville, calibri, serif"), S::Size(22.)]);
    db.add_text("According to Wikipedia, the foremost expert on any subject,\n\n");
    db.leave_span();
    db.enter_span(&[S::Weight(Weight::BOLD)]);
    db.add_text("Typography");
    db.leave_span();
    db.add_text(" is the ");
    db.enter_span(&[S::Style(Style::Italic)]);
    db.add_text("art and technique");
    db.leave_span();
    db.add_text(" of arranging type to make ");
    db.enter_span(underline);
    db.add_text("written language");
    db.leave_span();
    db.add_text(" ");
    db.enter_span(underline);
    db.add_text("legible");
    db.leave_span();
    db.add_text(", ");
    db.enter_span(underline);
    db.add_text("readable");
    db.leave_span();
    db.add_text(" and ");
    db.enter_span(underline);
    db.add_text("appealing");
    db.leave_span();
    db.add_text(WIKI_TYPOGRAPHY_REST);
    db.enter_span(&[S::LineSpacing(1.)]);
    db.add_text(" Furthermore, العربية نص جميل. द क्विक ब्राउन फ़ॉक्स jumps over the lazy 🐕.\n\n");
    db.leave_span();
    db.enter_span(&[S::family_list("Menlo"), S::LineSpacing(1.)]);
    db.add_text("A true ❯");
    db.enter_span(&[S::Size(22.)]);
    db.add_text("🕵🏽‍♀️");
    db.leave_span();
    db.add_text(" will spot the tricky selection in this BiDi text: ");
    db.enter_span(&[S::Size(22.)]);
    db.add_text("ניפגש ב09:35 בחוף הים");
    db.leave_span();
    db.build()
}

const WIKI_TYPOGRAPHY_REST: &'static str = " when displayed. The arrangement of type involves selecting typefaces, point sizes, line lengths, line-spacing (leading), and letter-spacing (tracking), and adjusting the space between pairs of letters (kerning). The term typography is also applied to the style, arrangement, and appearance of the letters, numbers, and symbols created by the process.";

