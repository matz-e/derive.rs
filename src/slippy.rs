use geo::algorithm::contains::Contains;
use geo_types::{Coord, Point, Rect};

pub const TILE_SIZE: u32 = 256;

/// Convert lon/lat coordinates to OSM tile coordinates of the given zoom level
pub fn to_tile(p: Point<f64>, zoom: u8) -> Point<f64> {
    let n = 2u32.pow(zoom as u32) as f64;
    let x = n * ((p.x() + 180.0) / 360.0);
    let y = n * (1.0 - (p.y().to_radians().tan().asinh() / std::f64::consts::PI)) * 0.5;
    (x, y).into()
}

/// Converts a coordinate in the OSM tile reference at the given zoom level to lon/lat
pub fn from_tile(p: Point<f64>, zoom: u8) -> Point<f64> {
    let n = 2u32.pow(zoom as u32) as f64;
    let x = p.x() / n * 360.0 - 180.0;
    let y = ((std::f64::consts::PI * (1.0 - 2.0 / n * p.y())).sinh())
        .atan()
        .to_degrees();
    (x, y).into()
}

/// A reference map with display size and lon/lat as well as OSM extends
#[derive(Clone, Copy)]
pub struct Map {
    /// Extends in tile coordinates
    extends_tiled: Rect<f64>,
    /// Extends in longitude/latitude
    extends_coord: Rect<f64>,
    /// Size in pixels
    size: Point<u32>,
    /// Zoom level of the current map
    zoom: u8,
}

impl Map {
    /// Coordinate extends
    pub fn extends(&self) -> Rect<f64> {
        self.extends_coord
    }

    pub fn from(center_x: f64, center_y: f64, width: u32, height: u32, zoom: u8) -> Self {
        let size = Point::new(width, height);
        let tile_extends = Point::new(size.x() as f64, size.y() as f64) / TILE_SIZE as f64;

        let center = Point::new(center_x, center_y);
        let center = to_tile(center, zoom);
        let extends_tiled = Rect::new(center + tile_extends * 0.5, center - tile_extends * 0.5);
        let extends_coord = Rect::new(
            from_tile(extends_tiled.min().into(), zoom),
            from_tile(extends_tiled.max().into(), zoom),
        );

        Self {
            extends_tiled,
            extends_coord,
            size,
            zoom,
        }
    }

    pub fn pixel_size(&self) -> (u32, u32) {
        (self.size.x(), self.size.y())
    }

    pub fn pixel_offsets(&self) -> (u32, u32) {
        let tile_min_x = self.extends_tiled.min().x;
        let tile_min_y = self.extends_tiled.min().y;
        let offset_x = ((tile_min_x - tile_min_x.trunc()) * TILE_SIZE as f64) as u32;
        let offset_y = ((tile_min_y - tile_min_y.trunc()) * TILE_SIZE as f64) as u32;
        (offset_x, offset_y)
    }

    pub fn tile_offsets(&self) -> (u32, u32) {
        (
            self.extends_tiled.min().x as u32,
            self.extends_tiled.min().y as u32,
        )
    }

    pub fn tile_xs(&self) -> std::ops::RangeInclusive<u32> {
        self.extends_tiled.min().x as u32..=self.extends_tiled.max().x as u32
    }

    pub fn tile_ys(&self) -> std::ops::RangeInclusive<u32> {
        self.extends_tiled.min().y as u32..=self.extends_tiled.max().y as u32
    }

    pub fn to_pixels(&self, coord: &Point<f64>) -> Option<Coord<u32>> {
        if !self.extends_coord.contains(coord) {
            return None;
        }
        let float_coord =
            (to_tile(*coord, self.zoom) - self.extends_tiled.min().into()) * TILE_SIZE.into();
        Some((float_coord.x() as u32, float_coord.y() as u32).into())
    }

    pub fn zoom(&self) -> u8 {
        self.zoom
    }
}
