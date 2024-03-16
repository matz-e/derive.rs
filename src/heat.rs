use fonts::system_fonts;
use geo_types::{Coord, Point};
use image::ImageBuffer;
use imageproc::drawing::draw_text_mut;
use palette::{Gradient, Hsv, IntoColor, Pixel, Srgb};
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

pub trait Heatmap {
    fn as_image(&self) -> image::DynamicImage;
    fn as_image_with_overlay(
        &self,
        name: &str,
        date: &chrono::DateTime<chrono::Utc>,
    ) -> image::DynamicImage;
    fn add_point(&mut self, point: &Coord<u32>);
    fn decay(&mut self, amount: u32);
    fn project_to_screen(&self, coord: &Point<f64>) -> Option<Coord<u32>>;
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
                    return [0u8, 0, 0];
                }

                let heat = (count as f64).log(self.max_value as f64);
                let rgb: Srgb = GRADIENT.get(heat as f32).into_color();
                rgb.into_format().into_raw()
            })
            .collect::<Vec<_>>();

        let size = (self.width * self.height * 3) as usize;
        let mut pixels = Vec::with_capacity(size);

        for pxls in color_map.iter() {
            pixels.extend_from_slice(&pxls[..]);
        }

        let buffer = ImageBuffer::from_raw(self.width, self.height, pixels).unwrap();
        image::DynamicImage::ImageRgb8(buffer)
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
        // FIXME: lol rust?
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
