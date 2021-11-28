extern crate directories;
extern crate http_req;
extern crate image;
extern crate sha2;
extern crate slippy_map_tilenames as smt;

use self::http_req::{request::Request, uri::Uri};
use self::sha2::{Digest, Sha256};

use std::convert::TryFrom;
use std::error::Error;
use std::fs::{create_dir_all, remove_file, File};
use std::path::{Path, PathBuf};

const TILE_SIZE: u32 = 256;

struct Downloader {
    cache_dir: PathBuf,
    url_pattern: String,
}

impl Downloader {
    fn new(url_pattern: &str) -> Result<Self, Box<dyn Error>> {
        Ok(Downloader {
            cache_dir: directories::BaseDirs::new()
                .unwrap()
                .cache_dir()
                .join("derive.rs")
                .join("tiles"),
            url_pattern: url_pattern.to_string(),
        })
    }

    fn get(&self, zoom: u8, x: u32, y: u32) -> Result<PathBuf, Box<dyn Error>> {
        let url = self
            .url_pattern
            .replace("{z}", &zoom.to_string())
            .replace("{x}", &x.to_string())
            .replace("{y}", &y.to_string());
        let hash = format!("{:X}", {
            let mut s = Sha256::new();
            s.update(&url);
            s.finalize()
        });
        let mut cached = self.cache_dir.join(Path::new(&hash));
        if let Some(ext) = Path::new(&url).extension() {
            cached = cached.with_extension(ext);
        } else {
            cached = cached.join(Path::new(".png"));
        }
        if cached.exists() {
            println!("cached: {}", url);
            return Ok(cached);
        }
        println!("fetch:  {}", url);
        if let Some(p) = cached.parent() {
            create_dir_all(p)?;
        }
        let mut writer = File::create(&cached)?;
        let uri = Uri::try_from(&url[..])?;
        match Request::new(&uri)
            .header("user-agent", "derive.rs 0.1 contact maps@sushinara.net")
            .send(&mut writer)
        {
            Ok(res) => {
                if !res.status_code().is_success() {
                    remove_file(cached)?;
                    Err(res.reason().into())
                } else {
                    Ok(cached)
                }
            }
            Err(e) => {
                remove_file(cached)?;
                Err(e.into())
            }
        }
    }
}

#[derive(Debug)]
pub struct Coord {
    lat: f64,
    lon: f64,
}

impl Coord {
    pub fn from(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

pub struct Basemap {
    top_left: Coord,
    bottom_right: Coord,
    height: u32,
    width: u32,
    offset_x: u32,
    offset_y: u32,
    zoom: u8,
    getter: Downloader,
}

impl Basemap {
    pub fn from(center: Coord, zoom: u8, width: u32, height: u32) -> Result<Self, Box<dyn Error>> {
        let (x, y) = smt::lonlat2tile(center.lon, center.lat, zoom);
        let center_tl = smt::tile2lonlat(x, y, zoom);
        let center_br = smt::tile2lonlat(x + 1, y + 1, zoom);

        let dx = center_br.0 - center_tl.0;
        let dy = center_tl.1 - center_br.1;

        let tile_extend_x = width as f64 / (2.0 * TILE_SIZE as f64);
        let tile_extend_y = height as f64 / (2.0 * TILE_SIZE as f64);

        let lat_min = (center.lat - tile_extend_y * dy).clamp(-90.0, 90.0);
        let lat_max = (center.lat + tile_extend_y * dy).clamp(-90.0, 90.0);

        let lon_min = (center.lon - tile_extend_x * dx).clamp(-180.0, 180.0);
        let lon_max = (center.lon + tile_extend_x * dx).clamp(-180.0, 180.0);

        let (outer_x, outer_y) = smt::lonlat2tile(lon_max, lat_max, zoom);
        let (outer_lon, outer_lat) = smt::tile2lonlat(outer_x, outer_y, zoom);

        let offset_x = ((outer_lon - lon_max).abs() * TILE_SIZE as f64 / dx) as u32;
        let offset_y = ((outer_lat - lat_max).abs() * TILE_SIZE as f64 / dy) as u32;

        Ok(Basemap {
            top_left: Coord::from(lat_max, lon_min),
            bottom_right: Coord::from(lat_min, lon_max),
            height,
            width,
            offset_x,
            offset_y,
            zoom,
            getter: Downloader::new("https://a.tile.osm.org/{z}/{x}/{y}.png")?,
        })
    }

    pub fn top_left_lat_lon(&self) -> (f64, f64) {
        (self.top_left.lat, self.top_left.lon)
    }

    pub fn bottom_right_lat_lon(&self) -> (f64, f64) {
        (self.bottom_right.lat, self.bottom_right.lon)
    }

    pub fn draw(self) -> Result<image::DynamicImage, Box<dyn Error>> {
        let mut pixmap = image::DynamicImage::new_rgba8(self.width, self.height);

        let map_tl = smt::lonlat2tile(self.top_left.lon, self.top_left.lat, self.zoom);
        let map_br = smt::lonlat2tile(self.bottom_right.lon, self.bottom_right.lat, self.zoom);
        for i in map_tl.0..=map_br.0 {
            for j in map_tl.1..=map_br.1 {
                let filename = self.getter.get(self.zoom, i, j)?;
                let raw_tile = image::open(filename)?;
                let mut tile = image::imageops::crop_imm(&raw_tile, 0, 0, TILE_SIZE, TILE_SIZE);
                let i = i - map_tl.0;
                let j = j - map_tl.1;
                let mut x = i * TILE_SIZE - self.offset_x;
                let mut y = j * TILE_SIZE - self.offset_y;
                if i == 0 && j == 0 {
                    x = 0;
                    y = 0;
                    tile = image::imageops::crop_imm(
                        &raw_tile,
                        self.offset_x,
                        0,
                        TILE_SIZE - self.offset_x,
                        TILE_SIZE - self.offset_y,
                    );
                } else if i == 0 {
                    x = 0;
                    tile = image::imageops::crop_imm(
                        &raw_tile,
                        self.offset_x,
                        0,
                        TILE_SIZE - self.offset_x,
                        TILE_SIZE,
                    );
                } else if j == 0 {
                    y = 0;
                    tile = image::imageops::crop_imm(
                        &raw_tile,
                        0,
                        self.offset_y,
                        TILE_SIZE,
                        TILE_SIZE - self.offset_y,
                    );
                }
                image::imageops::overlay(&mut pixmap, &tile, x, y);
            }
        }
        Ok(pixmap)
    }
}
