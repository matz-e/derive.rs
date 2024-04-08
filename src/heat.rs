use fonts::system_fonts;
use geo_types::{coord, Coord, Point};
use image::ImageBuffer;
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use palette::{Gradient, Hsv};
use rayon::prelude::*;
use rusttype::{Font, Scale};

use super::slippy;

lazy_static! {
    static ref GRADIENT: Gradient<Hsv> =
        Gradient::new(vec![Hsv::new(0.0, 0.75, 0.45), Hsv::new(0.0, 0.75, 1.00),]);
    static ref FONT: Font<'static> = {
        let property = system_fonts::FontPropertyBuilder::new()
            .family("Roboto Light")
            .build();
        if let Some((font_data, _)) = system_fonts::get(&property) {
            Font::try_from_vec(font_data).unwrap()
        } else {
            panic!("Cannot load font");
        }
    };
}

/// A representation of a heatmap
pub trait Heatmap: Send + Sync {
    /// Renders the heatmap
    fn as_image(&self) -> image::DynamicImage;

    /// Renders the heatmap with additional information
    /// TODO should be moved outside this trait
    fn as_image_with_overlay(
        &self,
        name: &str,
        date: &chrono::DateTime<chrono::Utc>,
    ) -> image::DynamicImage;

    /// Adds a point to the heatmap
    fn add_point(&mut self, point: &Coord<u32>);

    /// Reduces the heatmap by the given amount
    fn decay(&mut self, amount: u32);

    /// Takes a coordinate and converts it into the heatmap's internal representation
    fn project_to_screen(&self, coord: &Point<f64>) -> Option<Coord<u32>>;
}

/// Heatmap based on OSM tiles
pub struct TileHeatmap {
    map: slippy::Map,
    heatmap: Vec<u32>,
    height: u32,
    width: u32,
    min: Coord<u32>,
    max: Coord<u32>,
    max_value: u32,
    zoom: u8,
}

impl TileHeatmap {
    /// Create a new heatamp given the reference map and zoom level
    pub fn from(map: slippy::Map, zoom: u8) -> Self {
        let extends = map.extends();
        let raw_min = slippy::to_tile(extends.min().into(), zoom);
        let raw_max = slippy::to_tile(extends.max().into(), zoom);

        let min = coord! { x: raw_min.x(), y: raw_max.y() };
        let max = coord! { x: raw_max.x(), y: raw_min.y() };

        let width = max.x.ceil() as u32 - min.x.floor() as u32;
        let height = max.y.ceil() as u32 - min.y.floor() as u32;
        let size = (width * height) as usize;

        Self {
            map,
            heatmap: vec![0; size],
            height,
            width,
            min: coord! { x: min.x.floor() as u32, y: min.y.floor() as u32 },
            max: coord! { x: max.x.ceil() as u32, y: max.y.ceil() as u32 },
            max_value: 0,
            zoom,
        }
    }

    #[inline]
    fn get_tile_mut(&mut self, point: &Coord<u32>) -> Option<&mut u32> {
        if self.min.x <= point.x
            && point.x < self.max.x
            && self.min.y <= point.y
            && point.y < self.max.y
        {
            let index = ((point.x - self.min.x) + ((point.y - self.min.y) * self.width)) as usize;
            return Some(&mut self.heatmap[index]);
        }
        None
    }

    /// Tile size on the projected map, in pixels
    fn get_tile_size(&self) -> u32 {
        let c1 = slippy::from_tile(
            Point::new(self.min.x as f64 + 1.0, self.min.y as f64 + 1.0),
            self.zoom,
        );
        let c2 = slippy::from_tile(
            Point::new(self.min.x as f64 + 2.0, self.min.y as f64 + 2.0),
            self.zoom,
        );
        let t1 = self.map.to_pixels(&c1).unwrap();
        let t2 = self.map.to_pixels(&c2).unwrap();
        t2.x - t1.x
    }

    /// Offset for the origin tile on the projected map, in pixels
    fn get_tile_offset(&self) -> (i32, i32) {
        let c1 = slippy::from_tile(
            Point::new(self.min.x as f64 + 1.0, self.min.y as f64 + 1.0),
            self.zoom,
        );
        let t1 = self.map.to_pixels(&c1).unwrap();
        let size = self.get_tile_size() as i32;
        (t1.x as i32 - size, t1.y as i32 - size)
    }
}

impl Heatmap for TileHeatmap {
    fn as_image(&self) -> image::DynamicImage {
        let (width, height) = self.map.pixel_size();
        let mut buffer = ImageBuffer::new(width, height);

        let (x0, y0) = self.get_tile_offset();
        let tile_size = self.get_tile_size();

        for x in 0..self.width {
            for y in 0..self.height {
                let count = self.heatmap[(x + y * self.width) as usize];
                let heat = if count > 0 {
                    (count as f64 + 1.0).log10() / (self.max_value as f64 + 1.0).log10() * 250.0
                        + 6.0
                } else {
                    0.0
                };
                let color = image::Rgba([heat as u8, 0, 0, heat as u8]);
                let pos = Rect::at(x0 + (x * tile_size) as i32, y0 + (y * tile_size) as i32)
                    .of_size(tile_size, tile_size);
                draw_filled_rect_mut(&mut buffer, pos, color);
            }
        }

        image::DynamicImage::ImageRgba8(buffer)
    }

    /// Not supported
    fn as_image_with_overlay(
        &self,
        _name: &str,
        _date: &chrono::DateTime<chrono::Utc>,
    ) -> image::DynamicImage {
        self.as_image()
    }

    #[inline]
    fn add_point(&mut self, point: &Coord<u32>) {
        let px = {
            let px = self.get_tile_mut(point).unwrap();
            *px += 1;
            *px
        };

        self.max_value = self.max_value.max(px);
    }

    #[allow(dead_code)]
    fn decay(&mut self, amount: u32) {
        self.max_value -= 1;

        self.heatmap.par_iter_mut().for_each(|px| {
            if *px > amount {
                *px -= amount;
            }
        });
    }

    // Returns None if point is off screen.
    fn project_to_screen(&self, coord: &Point<f64>) -> Option<Coord<u32>> {
        let raw_mapped = slippy::to_tile(*coord, self.zoom);
        let x = raw_mapped.x().floor() as u32;
        let y = raw_mapped.y().floor() as u32;
        if self.min.x <= x && x < self.max.x && self.min.y <= y && y < self.max.y {
            return Some(coord! { x: x, y: y });
        }
        None
    }
}

pub struct PixelHeatmap {
    map: slippy::Map,
    heatmap: Vec<u32>,
    height: u32,
    width: u32,
    max_value: u32,
    render_date: bool,
    render_title: bool,
}

impl PixelHeatmap {
    pub fn from(map: slippy::Map, render_date: bool, render_title: bool) -> Self {
        let (width, height) = map.pixel_size();
        let size = (width * height) as usize;

        Self {
            map,
            heatmap: vec![0; size],
            height,
            width,
            max_value: 0,
            render_date,
            render_title,
        }
    }

    #[inline]
    fn get_pixel_mut(&mut self, point: &Coord<u32>) -> Option<&mut u32> {
        if point.x >= self.width || point.y >= self.height {
            return None;
        }

        let index = (point.x + (point.y * self.width)) as usize;
        Some(&mut self.heatmap[index])
    }
}

impl Heatmap for PixelHeatmap {
    fn as_image(&self) -> image::DynamicImage {
        let color_map = self
            .heatmap
            .clone()
            .into_par_iter()
            .map(|count| {
                if count == 0 {
                    return [0u8, 0, 0, 0];
                }

                let heat = ((count as f64 + 1.0).log10() / (self.max_value as f64 + 1.0).log10()
                    * 250.0
                    + 6.0) as u8;

                [heat, 0, 0, heat]
            })
            .collect::<Vec<_>>();

        let size = (self.width * self.height * 4) as usize;
        let mut pixels = Vec::with_capacity(size);

        for pxls in color_map.iter() {
            pixels.extend_from_slice(&pxls[..]);
        }

        let buffer = ImageBuffer::from_raw(self.width, self.height, pixels).unwrap();
        image::DynamicImage::ImageRgba8(buffer)
    }

    fn as_image_with_overlay(
        &self,
        name: &str,
        date: &chrono::DateTime<chrono::Utc>,
    ) -> image::DynamicImage {
        let mut image = self.as_image();

        let white = image::Rgba([255; 4]);
        let scale = Scale::uniform(self.height as f32 / 15.0);

        let x = 20;
        let mut y = self.height - scale.y as u32;

        if self.render_date {
            let date_string = date.format("%B %d, %Y").to_string();
            draw_text_mut(&mut image, white, x, y, scale, &FONT, date_string.as_str());
            y -= scale.y as u32;
        }

        if self.render_title {
            draw_text_mut(&mut image, white, x, y, scale, &FONT, name);
        }

        image
    }

    #[inline]
    fn add_point(&mut self, point: &Coord<u32>) {
        let px = {
            let px = self.get_pixel_mut(point).unwrap();
            *px += 1;
            *px
        };

        self.max_value = self.max_value.max(px);
    }

    #[allow(dead_code)]
    fn decay(&mut self, amount: u32) {
        self.max_value -= 1;

        self.heatmap.par_iter_mut().for_each(|px| {
            if *px > amount {
                *px -= amount;
            }
        });
    }

    // Returns None if point is off screen.
    fn project_to_screen(&self, coord: &Point<f64>) -> Option<Coord<u32>> {
        if let Some(mapping) = self.map.to_pixels(coord) {
            return Some(mapping);
        }
        None
    }
}
