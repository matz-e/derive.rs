use http_req::{request::Request, uri::Uri};
use sha2::{Digest, Sha256};

use std::convert::TryFrom;
use std::error::Error;
use std::path::{Path, PathBuf};

use super::slippy;

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
            return Ok(cached);
        }
        if let Some(p) = cached.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut writer = Vec::new();
        let uri = Uri::try_from(&url[..])?;
        match Request::new(&uri)
            .header("user-agent", "derive.rs 0.1 contact maps@sushinara.net")
            .send(&mut writer)
        {
            Ok(res) => {
                if !res.status_code().is_success() {
                    let msg = format!("failed to get {}: {}", url, res.reason());
                    Err(msg.into())
                } else {
                    std::fs::write(&cached, writer)?;
                    Ok(cached)
                }
            }
            Err(e) => Err(e.into()),
        }
    }
}

/// A basemap displaying OSM tiles
pub struct Basemap {
    map: slippy::Map,
    getter: Downloader,
}

impl Basemap {
    /// Create a basemap with specified map settings and tile download URL
    pub fn from(map: slippy::Map, url_pattern: &str) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            map,
            getter: Downloader::new(url_pattern)?,
        })
    }

    /// Download tile images and construct the basemap, tinting it with 1.0 being a fully black
    /// map.
    pub fn as_image(&self, tint: f32) -> Result<image::DynamicImage, Box<dyn Error>> {
        let (width, height) = self.map.pixel_size();
        let mut pixmap = image::DynamicImage::new_rgba8(width, height);

        let (offset_x, offset_y) = self.map.pixel_offsets();
        let (tile_min_x, tile_min_y) = self.map.tile_offsets();

        for i in self.map.tile_xs() {
            for j in self.map.tile_ys() {
                let filename = self.getter.get(self.map.zoom(), i, j)?;
                let raw_tile = image::open(filename)?;
                let mut tile = image::imageops::crop_imm(
                    &raw_tile,
                    0,
                    0,
                    slippy::TILE_SIZE,
                    slippy::TILE_SIZE,
                );

                let i = i - tile_min_x;
                let j = j - tile_min_y;
                let mut x = i * slippy::TILE_SIZE - offset_x;
                let mut y = j * slippy::TILE_SIZE - offset_y;

                if i == 0 && j == 0 {
                    x = 0;
                    y = 0;
                    tile = image::imageops::crop_imm(
                        &raw_tile,
                        offset_x,
                        offset_y,
                        slippy::TILE_SIZE - offset_x,
                        slippy::TILE_SIZE - offset_y,
                    );
                } else if i == 0 {
                    x = 0;
                    tile = image::imageops::crop_imm(
                        &raw_tile,
                        offset_x,
                        0,
                        slippy::TILE_SIZE - offset_x,
                        slippy::TILE_SIZE,
                    );
                } else if j == 0 {
                    y = 0;
                    tile = image::imageops::crop_imm(
                        &raw_tile,
                        0,
                        offset_y,
                        slippy::TILE_SIZE,
                        slippy::TILE_SIZE - offset_y,
                    );
                }
                image::imageops::overlay(&mut pixmap, &tile, x, y);
            }
        }
        let mut tint_layer = image::DynamicImage::new_rgba8(width, height);
        let color = image::Rgba([0u8, 0, 0, (tint.clamp(0.0, 1.0) * 255.0) as u8]);
        let fullscreen = imageproc::rect::Rect::at(0, 0).of_size(width, height);
        imageproc::drawing::draw_filled_rect_mut(&mut tint_layer, fullscreen, color);
        image::imageops::overlay(&mut pixmap, &tint_layer, 0, 0);
        Ok(pixmap)
    }
}
