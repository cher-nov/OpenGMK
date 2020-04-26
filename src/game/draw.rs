use crate::{
    game::{Game, GetAsset},
    gml,
};
use std::{cmp::Ordering, hint::unreachable_unchecked};

#[derive(Clone, Copy)]
pub enum Halign {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy)]
pub enum Valign {
    Top,
    Middle,
    Bottom,
}

impl Game {
    /// Draws all instances, tiles and backgrounds to the screen, taking all active views into account.
    /// Note that this function runs GML code associated with object draw events, so its usage must match GameMaker 8.
    pub fn draw(&mut self) -> gml::Result<()> {
        // Draw all views
        if self.views_enabled {
            // Iter views in a non-borrowing way
            let mut count = 0;
            while let Some(&view) = self.views.get(count) {
                if view.visible {
                    self.view_current = count;
                    self.draw_view(
                        view.source_x,
                        view.source_y,
                        view.source_w as _,
                        view.source_h as _,
                        view.port_x,
                        view.port_y,
                        view.port_w as _,
                        view.port_h as _,
                        view.angle,
                    )?;
                }
                count += 1;
            }
            self.view_current = 0;
        } else {
            self.draw_view(0, 0, self.room_width, self.room_height, 0, 0, self.room_width, self.room_height, 0.0)?;
        }

        // Tell renderer to finish the frame and start the next one
        let (width, height) = self.window.inner_size().into();
        self.renderer.finish(width, height);

        // Clear inputs for this frame
        self.input_manager.clear_presses();

        Ok(())
    }

    /// Draws everything in the scene using a given view rectangle
    fn draw_view(
        &mut self,
        src_x: i32,
        src_y: i32,
        src_w: i32,
        src_h: i32,
        port_x: i32,
        port_y: i32,
        port_w: i32,
        port_h: i32,
        angle: f64,
    ) -> gml::Result<()> {
        let (width, height) = self.window.inner_size().into();
        self.renderer.set_view(
            width,
            height,
            self.unscaled_width,
            self.unscaled_height,
            src_x,
            src_y,
            src_w,
            src_h,
            angle.to_radians(),
            port_x,
            port_y,
            port_w,
            port_h,
        );

        fn draw_instance(game: &mut Game, idx: usize) -> gml::Result<()> {
            let instance = game.instance_list.get(idx).unwrap_or_else(|| unsafe { unreachable_unchecked() });
            if game.custom_draw_objects.contains(&instance.object_index.get()) {
                // Custom draw event
                game.run_instance_event(gml::ev::DRAW, 0, idx, idx, None)
            } else {
                // Default draw action
                if let Some(Some(sprite)) = game.assets.sprites.get(instance.sprite_index.get() as usize) {
                    let image_index = instance.image_index.get().floor() as i32 % sprite.frames.len() as i32;
                    let atlas_ref = match sprite.frames.get(image_index as usize) {
                        Some(f1) => &f1.atlas_ref,
                        None => return Ok(()), // sprite with 0 frames?
                    };
                    game.renderer.draw_sprite(
                        atlas_ref,
                        instance.x.get(),
                        instance.y.get(),
                        instance.image_xscale.get(),
                        instance.image_yscale.get(),
                        instance.image_angle.get(),
                        instance.image_blend.get(),
                        instance.image_alpha.get(),
                    )
                }
                Ok(())
            }
        }

        fn draw_tile(game: &mut Game, idx: usize) {
            let tile = game.tile_list.get(idx).unwrap_or_else(|| unsafe { unreachable_unchecked() });
            if let Some(Some(background)) = game.assets.backgrounds.get(tile.background_index as usize) {
                if let Some(atlas) = &background.atlas_ref {
                    game.renderer.draw_sprite_partial(
                        atlas,
                        tile.tile_x as _,
                        tile.tile_y as _,
                        tile.width as _,
                        tile.height as _,
                        tile.x,
                        tile.y,
                        tile.xscale,
                        tile.yscale,
                        0.0,
                        tile.blend,
                        tile.alpha,
                    )
                }
            }
        }

        // draw backgrounds
        for background in self.backgrounds.iter().filter(|x| x.visible && !x.is_foreground) {
            if let Some(atlas_ref) =
                self.assets.backgrounds.get_asset(background.background_id).and_then(|x| x.atlas_ref.as_ref())
            {
                self.renderer.draw_sprite(
                    atlas_ref,
                    background.x_offset,
                    background.y_offset,
                    background.xscale,
                    background.yscale,
                    0.0,
                    background.blend,
                    background.alpha,
                );
            }
        }

        self.instance_list.draw_sort();
        let mut iter_inst = self.instance_list.iter_by_drawing();
        let mut iter_inst_v = iter_inst.next(&self.instance_list);
        self.tile_list.draw_sort();
        let mut iter_tile = self.tile_list.iter_by_drawing();
        let mut iter_tile_v = iter_tile.next(&self.tile_list);
        loop {
            match (iter_inst_v, iter_tile_v) {
                (Some(idx_inst), Some(idx_tile)) => {
                    let inst = self.instance_list.get(idx_inst).unwrap_or_else(|| unsafe { unreachable_unchecked() });
                    let tile = self.tile_list.get(idx_tile).unwrap_or_else(|| unsafe { unreachable_unchecked() });
                    match inst.depth.get().cmp(&tile.depth) {
                        Ordering::Greater | Ordering::Equal => {
                            draw_instance(self, idx_inst)?;
                            iter_inst_v = iter_inst.next(&self.instance_list);
                        },
                        Ordering::Less => {
                            draw_tile(self, idx_tile);
                            iter_tile_v = iter_tile.next(&self.tile_list);
                        },
                    }
                },
                (Some(idx_inst), None) => {
                    draw_instance(self, idx_inst)?;
                    while let Some(idx_inst) = iter_inst.next(&self.instance_list) {
                        draw_instance(self, idx_inst)?;
                    }
                    break
                },
                (None, Some(idx_tile)) => {
                    draw_tile(self, idx_tile);
                    while let Some(idx_tile) = iter_tile.next(&self.tile_list) {
                        draw_tile(self, idx_tile);
                    }
                    break
                },
                (None, None) => break,
            }
        }

        // draw foregrounds
        for background in self.backgrounds.iter().filter(|x| x.visible && x.is_foreground) {
            if let Some(atlas_ref) =
                self.assets.backgrounds.get_asset(background.background_id).and_then(|x| x.atlas_ref.as_ref())
            {
                self.renderer.draw_sprite(
                    atlas_ref,
                    background.x_offset,
                    background.y_offset,
                    background.xscale,
                    background.yscale,
                    0.0,
                    background.blend,
                    background.alpha,
                );
            }
        }

        Ok(())
    }

    /// Gets width and height of a string using the current draw_font.
    /// If line_height is None, a line height will be inferred from the font.
    /// If max_width is None, the string will not be given a maximum width.
    pub fn get_string_size(&self, string: &str, line_height: Option<u32>, max_width: Option<u32>) -> (u32, u32) {
        let font = self.draw_font.as_ref().unwrap();
        let mut width = 0;
        let mut height = 0;
        let mut current_line_width = 0;

        // Figure out what the height of a line is if one wasn't specified
        // GM8 does this by getting the height of 'M'. Yeah. I don't know either.
        let line_height = match line_height {
            Some(h) => h,
            None => font.get_char(u32::from('M')).map(|x| x.height).unwrap_or_default(),
        };

        let mut iter = string.chars().peekable();
        while let Some(c) = iter.next() {
            // First, get the next character we're going to be processing
            let character = match c {
                '#' => {
                    // '#' is a newline character, don't process it but start a new line instead
                    height += line_height;
                    if current_line_width > width {
                        width = current_line_width;
                    }
                    current_line_width = 0;
                    continue
                },
                '\\' if iter.peek() == Some(&'#') => {
                    // '\#' is an escaped newline character, treat it as '#'
                    iter.next(); // consumes '#'
                    match font.get_char(u32::from('#')) {
                        // consumes '#'
                        Some(character) => character,
                        None => continue,
                    }
                },
                _ => {
                    // Normal character
                    match font.get_char(u32::from(c)) {
                        Some(character) => character,
                        None => continue,
                    }
                },
            };

            // Check if we're going over the max width
            if let Some(max_width) = max_width {
                if current_line_width + character.width > max_width && current_line_width != 0 {
                    height += line_height;
                    if current_line_width > width {
                        width = current_line_width;
                    }
                    current_line_width = 0;
                }
            }

            // Apply current character to line width
            current_line_width += character.distance;
        }

        // Pretend there's a newline at the end
        height += line_height;
        if current_line_width > width {
            width = current_line_width;
        }

        (width, height)
    }

    /// Draws a string to the screen at the given coordinates.
    /// If line_height is None, a line height will be inferred from the font.
    /// If max_width is None, the string will not be given a maximum width.
    pub fn draw_string(&mut self, x: i32, y: i32, string: &str, line_height: Option<u32>, max_width: Option<u32>) {
        let font = self.draw_font.as_ref().unwrap();

        // Figure out what the height of a line is if one wasn't specified
        // GM8 does this by getting the height of 'M'. Yeah. I don't know either.
        let line_height = match line_height {
            Some(h) => h as i32,
            None => font.get_char(u32::from('M')).map(|x| x.height).unwrap_or_default() as i32,
        };

        // Figure out where the cursor should start based on our font align variables.
        let (mut cursor_x, mut cursor_y) = match (self.draw_halign, self.draw_valign) {
            (Halign::Left, Valign::Top) => (x, y), // avoids calling get_string_size if we don't need to
            (h, v) => {
                let (width, height) = self.get_string_size(string, None, None);
                (
                    match h {
                        Halign::Left => x,
                        Halign::Middle => x - (width as i32 / 2),
                        Halign::Right => x - width as i32,
                    },
                    match v {
                        Valign::Top => y,
                        Valign::Middle => y - (height as i32 / 2),
                        Valign::Bottom => y - height as i32,
                    },
                )
            },
        };
        let start_x = cursor_x;

        // Iterate the characters in the string so we can draw them
        let mut iter = string.chars().peekable();
        while let Some(c) = iter.next() {
            // First, get the next character we're going to be processing
            let character = match c {
                '#' => {
                    // '#' is a newline character, don't process it but start a new line instead
                    cursor_x = start_x;
                    cursor_y += line_height;
                    continue
                },
                '\\' if iter.peek() == Some(&'#') => {
                    // '\#' is an escaped newline character, treat it as '#'
                    iter.next(); // consumes '#'
                    match font.get_char(u32::from('#')) {
                        // consumes '#'
                        Some(character) => character,
                        None => continue,
                    }
                },
                _ => {
                    // Normal character
                    match font.get_char(u32::from(c)) {
                        Some(character) => character,
                        None => continue,
                    }
                },
            };

            // Check if we're going over max width
            // Check if we're going over the max width
            if let Some(max_width) = max_width {
                let line_width = (cursor_x - start_x) as u32;
                if line_width + character.width > max_width && line_width != 0 {
                    cursor_x = start_x;
                    cursor_y += line_height;
                }
            }

            // Draw the character to the screen
            self.renderer.draw_sprite_partial(
                &font.atlas_ref,
                character.x as _,
                character.y as _,
                character.width as _,
                character.height as _,
                (character.offset as i32 + cursor_x).into(),
                cursor_y.into(),
                1.0,
                1.0,
                0.0,
                u32::from(self.draw_colour) as i32,
                self.draw_alpha,
            );

            // Move cursor
            cursor_x += character.distance as i32;
        }
    }
}