extern crate chrono;
extern crate docopt;
extern crate font_loader as fonts;
extern crate geo;
extern crate gpx;
extern crate image;
extern crate imageproc;
#[macro_use]
extern crate lazy_static;
extern crate libc;
extern crate palette;
extern crate rayon;
extern crate rusttype;
extern crate serde;

mod osmbase;

use osmbase::{Basemap, Coord};

use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::{stdout, BufReader};
use std::path;

use docopt::Docopt;
use fonts::system_fonts;
use gpx::read;
use gpx::{Gpx, Track};
use geo::Point;
use palette::{Gradient, Hsv, IntoColor, Pixel, Srgb};
use image::ImageBuffer;
use imageproc::drawing::draw_text_mut;
use rayon::prelude::*;
use rusttype::{Font, Scale};
use serde::Deserialize;

const USAGE: &'static str = "
Generate video from GPX files.

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

Video options:
  -r, --frame-rate=RATE  Output a frame every `RATE` GPS points [default: 1500]
  -s, --ppm-stream       Output a PPM stream to stdout.
  --title                Render activity title into each frame.
  --date                 Render activity date into each frame.
";

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
    // video options
    flag_frame_rate: u32,
    flag_ppm_stream: bool,
    flag_title: bool,
    flag_date: bool,
}

type ScreenPoint = (u32, u32);

lazy_static!{
    static ref GRADIENT: Gradient<Hsv> =
        Gradient::new(
        vec![
            Hsv::new(0.0, 0.75, 0.20),
            Hsv::new(0.0, 0.75, 1.00),
        ]);

    static ref FONT: Font<'static> = {
        let property = system_fonts::FontPropertyBuilder::new().family("Roboto Light").build();
        if let Some((font_data, _)) = system_fonts::get(&property) {
            Font::try_from_vec(font_data).unwrap()
        } else {
            panic!("Cannot load font");
        }
    };
}

struct Heatmap {
    top_left: Point<f64>,
    scale: Point<f64>,
    width: u32,
    height: u32,
    heatmap: Vec<u32>,
    max_value: u32,
    render_date: bool,
    render_title: bool,
}

impl Heatmap {
    pub fn from(
        tl: (f64, f64),
        br: (f64, f64),
        args: &CommandArgs
    ) -> Heatmap {
        let top_left = Point::new(tl.1, tl.0);
        let bot_right = Point::new(br.1, br.0);
        let size = (args.flag_width * args.flag_height) as usize;

        let mut heatmap = Vec::with_capacity(size);
        for _ in 0..size {
            heatmap.push(0);
        }
        let scale = Point::new(1.0 / (top_left.lng() - bot_right.lng()), 1.0 / (top_left.lat() - bot_right.lat()));

        Heatmap {
            top_left: top_left,
            scale: scale,
            width: args.flag_width,
            height: args.flag_height,
            heatmap: heatmap,
            max_value: 0,
            render_date: args.flag_date,
            render_title: args.flag_title,
        }
    }

    pub fn as_image(&self) -> image::DynamicImage {
        let color_map = self.heatmap
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

    pub fn as_image_with_overlay(&self, act: &Activity) -> image::DynamicImage {
        let mut image = self.as_image();

        let white = image::Rgba([255; 4]);
        let scale = Scale::uniform(self.height as f32 / 15.0);

        let x = 20;
        let mut y = self.height - scale.y as u32;

        if self.render_date {
            let date_string = act.date.format("%B %d, %Y").to_string();
            draw_text_mut(&mut image, white, x, y as i32, scale, &FONT, date_string.as_str());
            y -= scale.y as u32;
        }

        if self.render_title {
            draw_text_mut(&mut image, white, x, y as i32, scale, &FONT, act.name.as_str());
        }

        image
    }

    #[inline]
    fn get_pixel_mut(&mut self, point: &ScreenPoint) -> Option<&mut u32> {
        if point.0 >= self.width || point.1 >= self.height {
            return None;
        }

        let index = (point.0 + (point.1 * self.width)) as usize;
        Some(&mut self.heatmap[index])
    }

    #[inline]
    pub fn add_point(&mut self, point: &ScreenPoint) {
        // FIXME: lol rust?
        let px = {
            let px = self.get_pixel_mut(point).unwrap();
            *px += 1;
            *px
        };

        self.max_value = self.max_value.max(px);
    }

    #[allow(dead_code)]
    pub fn decay(&mut self, amount: u32) {
        self.max_value -= 1;

        self.heatmap.par_iter_mut().for_each(|px| {
            if *px > amount {
                *px -= amount;
            }
        });
    }

    // Using simple equirectangular projection for now. Returns None if point
    // is off screen.
    pub fn project_to_screen(&self, coord: &Point<f64>) -> Option<ScreenPoint> {
        // lng is x pos
        let x_pos = self.top_left.lng() - coord.lng();
        let y_pos = self.top_left.lat() - coord.lat();

        let x_offset = x_pos * self.scale.lng();
        let y_offset = y_pos * self.scale.lat();

        let (x, y) = (
            (x_offset * self.width as f64),
            (y_offset * self.height as f64),
        );

        if (x < 0.0 || x as u32 >= self.width) || (y < 0.0 || y as u32 >= self.height) {
            None
        } else {
            Some((x as u32, y as u32))
        }
    }
}

#[derive(Debug)]
struct Activity {
    name: String,
    date: chrono::DateTime<chrono::Utc>,
    track_points: Vec<Point<f64>>,
}

fn parse_gpx(path: &path::PathBuf) -> Result<Activity, Box<dyn Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let gpx: Gpx = read(reader)?;

    // Nothing to do if there are no tracks
    if gpx.tracks.len() == 0 {
        return Err(Box::from("file has no tracks"));
    } else if gpx.tracks.len() > 1 {
        eprintln!("Warning! more than 1 track, just taking first");
    }

    let track: &Track = &gpx.tracks[0];

    let mut activity = Activity {
        name: track.name.clone().unwrap_or(String::from("Untitled")),
        date: chrono::Utc::now(),
        track_points: vec![],
    };

    if let Some(metadata) = gpx.metadata {
        if let Some(time) = metadata.time {
            activity.date = time;
        }
    }

    // Append all the waypoints.
    for seg in track.segments.iter() {
        let points = seg.points.iter().map(|ref wpt| wpt.point());
        activity.track_points.extend(points);
    }

    if activity.track_points.len() == 0 {
        Err(Box::from("No track points"))
    } else {
        Ok(activity)
    }
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

    let basemap = Basemap::from(Coord::from(args.flag_lat, args.flag_lon), args.flag_zoom, args.flag_width, args.flag_height)?;

    let mut map = Heatmap::from(basemap.top_left_lat_lon(), basemap.bottom_right_lat_lon(), &args);
    let output_dir = match fs::read_dir(args.arg_directory) {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("Error reading input directory: {}", err);
            std::process::exit(1);
        }
    };

    let paths: Vec<path::PathBuf> = output_dir.into_iter().map(|p| p.unwrap().path()).collect();

    eprint!("Parsing {:?} GPX files...", paths.len());

    let mut activities: Vec<Activity> = paths
        .into_par_iter()
        .filter_map(|ref p| parse_gpx(p).ok())
        .collect();

    activities.sort_by_key(|a| a.date);

    eprintln!("Done!");

    let mut stdout = stdout();

    let mut counter;
    for act in activities {
        eprintln!("Activity: {}", act.name);

        let pixels: Vec<ScreenPoint> = act.track_points
            .par_iter()
            .filter_map(|ref pt| map.project_to_screen(pt))
            .collect();

        counter = 0;
        for ref point in pixels.into_iter() {
            map.add_point(point);

            counter += 1;

            if args.flag_ppm_stream && counter % args.flag_frame_rate == 0 {
                let image = map.as_image_with_overlay(&act);
                image.write_to(&mut stdout, image::ImageFormat::Pnm).unwrap();
            }
        }

        // FIXME: this is pretty ugly.
        // map.decay(1);
    }

    if args.flag_ppm_stream {
        map.as_image().write_to(&mut stdout, image::ImageFormat::Pnm).unwrap();
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
