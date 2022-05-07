extern crate chrono;
extern crate derivers;
extern crate docopt;
extern crate geo;
extern crate indicatif;
extern crate libc;
extern crate rayon;
extern crate serde;

use derivers::activity::{Activity, ScreenActivity};
use derivers::heat::Heatmap;
use derivers::osmbase::Basemap;
use derivers::slippy;

use std::error::Error;
use std::fs;
use std::io::stdout;
use std::path;

use docopt::Docopt;
use geo::Point;
use indicatif::ParallelProgressIterator;
use rayon::iter::ParallelIterator;
use rayon::prelude::*;
use serde::Deserialize;

const USAGE: &str = r#"
Generate video from GPX or FIT files.

Usage:
  derivers --lat=LAT --lon=LON [options] <directory>
  derivers (-h|--help)

Arguments:
  lat                    Latitude of the view port center
  lon                    Longitude of the view port center

Options:
  -h, --help             Show this help text.
  --lat=LAT              Latitude of the view port center
  --lon=LON              Longitude of the view port center
  --width=WIDTH          Width of output, in pixels [default: 1920]
  --height=HEIGHT        Height of output to pixel size [default: 1080]
  -o, --output=FILE      Output a PNG of cumulative heatmap data to file. [default: heatmap.png]
  -z, --zoom=LEVEL       Zoom level [default: 10]
  --url=URL              URL pattern for background tiles (standard OSM: https://a.tile.osm.org/{z}/{x}/{y}.png)
                         [default: https://stamen-tiles.a.ssl.fastly.net/terrain/{z}/{x}/{y}.png]

Video options:
  -r, --frame-rate=RATE  Output a frame every `RATE` GPS points [default: 1500]
  -s, --ppm-stream       Output a PPM stream to stdout.
  --title                Render activity title into each frame.
  --date                 Render activity date into each frame.
"#;

#[derive(Debug, Deserialize)]
struct CommandArgs {
    arg_directory: String,
    flag_help: bool,
    // general options
    flag_lat: f64,
    flag_lon: f64,
    flag_output: String,
    flag_width: u32,
    flag_height: u32,
    flag_zoom: u8,
    flag_url: String,
    // video options
    flag_frame_rate: u32,
    flag_ppm_stream: bool,
    flag_title: bool,
    flag_date: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: CommandArgs = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    if args.flag_help {
        eprintln!("{}", USAGE);
        return Ok(());
    }

    let is_tty = unsafe { libc::isatty(libc::STDOUT_FILENO as i32) } != 0;
    if args.flag_ppm_stream && is_tty {
        eprintln!(
            "Refusing to write frame data to TTY.\n
Please pipe output to a file or program."
        );
        std::process::exit(1);
    }

    let reference_map = slippy::Map::from(
        Point::new(args.flag_lon, args.flag_lat),
        Point::new(args.flag_width, args.flag_height),
        args.flag_zoom,
    );

    let basemap = Basemap::from(reference_map, &args.flag_url)?;
    let mut map = Heatmap::from(reference_map, args.flag_date, args.flag_title);
    let output_dir = match fs::read_dir(args.arg_directory) {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("Error reading input directory: {}", err);
            std::process::exit(1);
        }
    };

    let paths: Vec<path::PathBuf> = output_dir.into_iter().map(|p| p.unwrap().path()).collect();

    let npaths = paths.len();
    eprint!("Parsing {:?} files...", npaths);

    let mut activities: Vec<ScreenActivity> = paths
        .into_par_iter()
        .progress_count(npaths as u64)
        .filter_map(|ref p| Activity::from(p).ok())
        .filter_map(|a| a.project_to_screen(&map).ok())
        .collect();

    activities.sort_by_key(|a| a.date);

    let mut stdout = stdout();
    for act in activities {
        let mut counter = 0;
        for ref point in act.track_points.into_iter() {
            map.add_point(point);

            counter += 1;

            if args.flag_ppm_stream && counter % args.flag_frame_rate == 0 {
                let image = map.as_image_with_overlay(&act.name, &act.date);
                image
                    .write_to(&mut stdout, image::ImageFormat::Pnm)
                    .unwrap();
            }
        }

        // FIXME: this is pretty ugly.
        // map.decay(1);
    }

    if args.flag_ppm_stream {
        map.as_image()
            .write_to(&mut stdout, image::ImageFormat::Pnm)
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
    base_pixmap.save(args.flag_output)?;
    Ok(())
}
