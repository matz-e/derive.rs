extern crate chrono;
extern crate clap;
extern crate derivers;
extern crate geo;
extern crate libc;
extern crate serde;

use derivers::heat::{Heatmap, PixelHeatmap, TileHeatmap};
use derivers::osmbase::Basemap;
use derivers::slippy;
use derivers::strava;

use std::error::Error;
use std::io::stdout;
use std::path;

use clap::{Parser, ValueEnum};

/// Ensure that a number represents a fraction within [0.0, 1.0]
fn fraction(s: &str) -> Result<f32, String> {
    if let Ok(num) = s.parse::<f32>() {
        if (0.0..=1.0).contains(&num) {
            Ok(num)
        } else {
            Err(format!("value not in [0.0, 1.0]: {}", num))
        }
    } else {
        Err(format!("cannot parse '{}'", s))
    }
}

/// Different heatmap representations: pixel-precise, or based on OSM tiles level 14 or 17
#[derive(Clone, Debug, ValueEnum)]
enum HeatmapKind {
    Pixel,
    Squadrat,
    Squadratino,
}

/// Generate a heatmap from activities
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Directory containing the activities
    directory: String,

    // general options
    /// Latitude of the view port center
    #[arg(long)]
    lat: f64,
    /// Longitude of the view port center
    #[arg(long)]
    lon: f64,
    /// Output a PNG of cumulative heatmap data to file.
    #[arg(short, long, default_value = "heatmap.png")]
    output: String,
    /// Width of output, in pixels
    #[arg(short, long, default_value_t = 1920)]
    width: u32,
    /// Height of output to pixel size
    #[arg(short, long, default_value_t = 1080)]
    height: u32,
    /// Zoom level
    #[arg(short, long, default_value_t = 10)]
    zoom: u8,
    /// URL pattern for background tiles (standard OSM: https://a.tile.osm.org/{z}/{x}/{y}.png)
    #[arg(long, default_value = "https://tile.openstreetmap.org/{z}/{x}/{y}.png")]
    url: String,

    /// Tint overlay over the basemap
    #[arg(long, value_parser = fraction, default_value_t = 0.8)]
    tint: f32,

    /// What kind of heatmap to generate
    #[arg(long, value_enum, default_value_t = HeatmapKind::Pixel)]
    heatmap: HeatmapKind,

    // video options
    /// Output a frame every `RATE` GPS points
    #[arg(short = 'r', long, default_value_t = 1500)]
    frame_rate: u32,
    /// Output a stream to stdout to be processed with, e.g., ffmpeg.
    #[arg(short, long)]
    stream: bool,
    /// Render activity title into each frame.
    #[arg(short, long)]
    title: bool,
    /// Render activity date into each frame.
    #[arg(short, long)]
    date: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    #[cfg(unix)]
    {
        let is_tty = unsafe { libc::isatty(libc::STDOUT_FILENO) } != 0;
        if args.stream && is_tty {
            eprintln!(
                "Refusing to write frame data to TTY.\n
    Please pipe output to a file or program."
            );
            std::process::exit(1);
        }
    }

    let reference_map = slippy::Map::from(args.lon, args.lat, args.width, args.height, args.zoom);
    let basemap = Basemap::from(reference_map, &args.url)?;
    let mut map: Box<dyn Heatmap + Send> = match args.heatmap {
        HeatmapKind::Pixel => Box::new(PixelHeatmap::from(reference_map, args.date, args.title)),
        HeatmapKind::Squadrat => Box::new(TileHeatmap::from(reference_map, 14)),
        HeatmapKind::Squadratino => Box::new(TileHeatmap::from(reference_map, 17)),
    };

    let export = strava::DataExport::new(&path::PathBuf::from(&args.directory))?;
    let activities = export.parse(&*map);
    let mut stdout = stdout();
    let mut counter = 0;
    let rendered_basemap = basemap.as_image(args.tint)?;
    for act in activities {
        for ref point in act.track_points.into_iter() {
            map.add_point(point);

            counter += 1;

            if args.stream && counter % args.frame_rate == 0 {
                let mut pixmap = rendered_basemap.clone();
                let heat_pixmap = map.as_image().to_rgba8();
                image::imageops::overlay(&mut pixmap, &heat_pixmap, 0, 0);
                pixmap.write_to(&mut stdout, image::ImageFormat::Png)?;
            }
        }

        // FIXME: this is pretty ugly.
        // map.decay(1);
    }

    let mut pixmap = rendered_basemap;
    let heat_pixmap = map.as_image().to_rgba8();
    image::imageops::overlay(&mut pixmap, &heat_pixmap, 0, 0);
    if args.stream {
        pixmap.write_to(&mut stdout, image::ImageFormat::Png)?;
    } else {
        pixmap.save(args.output)?;
    }
    Ok(())
}
