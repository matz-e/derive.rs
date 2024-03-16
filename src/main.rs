extern crate chrono;
extern crate clap;
extern crate derivers;
extern crate geo;
extern crate libc;
extern crate serde;

use derivers::heat::{Heatmap, PixelHeatmap};
use derivers::osmbase::Basemap;
use derivers::slippy;
use derivers::strava;

use std::error::Error;
use std::io::stdout;
use std::path;

use clap::Parser;

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
    #[arg(
        long,
        default_value = "https://stamen-tiles.a.ssl.fastly.net/terrain/{z}/{x}/{y}.png"
    )]
    url: String,

    // video options
    /// Output a frame every `RATE` GPS points [default: 1500]
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
    let mut map = PixelHeatmap::from(reference_map, args.date, args.title);

    let export = strava::DataExport::new(&path::PathBuf::from(&args.directory))?;
    let activities = export.parse(&mut map);
    let mut stdout = stdout();
    for act in activities {
        let mut counter = 0;
        for ref point in act.track_points.into_iter() {
            map.add_point(point);

            counter += 1;

            if args.stream && counter % args.frame_rate == 0 {
                let image = map.as_image_with_overlay(&act.name, &act.date);
                image
                    .write_to(&mut stdout, image::ImageFormat::Png)
                    .unwrap();
            }
        }

        // FIXME: this is pretty ugly.
        // map.decay(1);
    }

    if args.stream {
        map.as_image()
            .write_to(&mut stdout, image::ImageFormat::Png)
            .unwrap();
    };
    let mut base_pixmap = basemap.draw()?;
    let mut heat_pixmap = map.as_image().to_rgba8();
    for pix in heat_pixmap.pixels_mut() {
        if pix[0] == 0 {
            pix[3] = 196;
        }
    }
    image::imageops::overlay(&mut base_pixmap, &heat_pixmap, 0, 0);
    base_pixmap.save(args.output)?;
    Ok(())
}
