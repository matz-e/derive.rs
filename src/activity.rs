use super::heat::Heatmap;

use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::io::BufReader;
use std::path;

use fitparser::profile::field_types;
use flate2::read::GzDecoder;
use gpx::{Gpx, Track};
use geo::{Coordinate, Point};
use rayon::prelude::*;
use rayon::iter::IntoParallelRefIterator;

fn parse_fit<T: std::io::Read>(reader: &mut BufReader<T>) -> Result<Activity, Box<dyn Error>> {
    let mut activity = Activity {
        name: "Untitled".to_string(),
        date: chrono::Utc::now(),
        track_points: vec![],
    };

    for data in fitparser::from_reader(reader)? {
        if data.kind() == field_types::MesgNum::Record {
            let mut lat: Option<f64> = None;
            let mut lon: Option<f64> = None;
            for field in data.fields() {
                if field.name() == "position_lat" {
                    if field.units() == "semicircles" {
                        if let fitparser::Value::SInt32(raw) = field.value() {
                            lat = Some(*raw as f64 * 180.0 / u32::pow(2, 31) as f64);
                        }
                    }
                } else if field.name() == "position_long" {
                    if field.units() == "semicircles" {
                        if let fitparser::Value::SInt32(raw) = field.value() {
                            lon = Some(*raw as f64 * 180.0 / u32::pow(2, 31) as f64);
                        }
                    }
                }
            }
            if let Some((x, y)) = lon.zip(lat) {
                activity.track_points.push(Point::new(x, y));
            }
        }
    }

    if activity.track_points.len() == 0 {
        Err(Box::from("No track points"))
    } else {
        Ok(activity)
    }
}

fn parse_gpx<T: std::io::Read>(reader: &mut BufReader<T>) -> Result<Activity, Box<dyn Error>> {
    let gpx: Gpx = gpx::read(reader)?;
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

fn parse<T: std::io::Read>(reader: &mut BufReader<T>, path: &path::PathBuf) -> Result<Activity, Box<dyn Error>> {
    if path.extension() == Some(OsStr::new("gpx")) {
        parse_gpx(reader)
    } else if path.extension() == Some(OsStr::new("fit")) {
        parse_fit(reader)
    } else {
        Err(Box::from("Unknown file type"))
    }
}

#[derive(Debug)]
pub struct Activity {
    name: String,
    date: chrono::DateTime<chrono::Utc>,
    track_points: Vec<Point<f64>>,
}

#[derive(Debug)]
pub struct ScreenActivity {
    pub name: String,
    pub date: chrono::DateTime<chrono::Utc>,
    pub track_points: Vec<Coordinate<u32>>,
}

impl Activity {
    pub fn from(path: &path::PathBuf) -> Result<Self, Box<dyn Error>> {
        if path.extension() == Some(OsStr::new("gz")) {
            let file = File::open(path)?;
            let decoder = GzDecoder::new(file);
            let mut reader = BufReader::new(decoder);
            parse(&mut reader, &path.with_extension(""))
        } else {
            let file = File::open(path)?;
            let mut reader = BufReader::new(file);
            parse(&mut reader, path)
        }
    }

    pub fn project_to_screen(self, heatmap: &Heatmap) -> Result<ScreenActivity, Box<dyn Error>> {
        let mut track_points: Vec<Coordinate<u32>> = self.track_points.par_iter()
                .filter_map(|ref pt| heatmap.project_to_screen(pt))
                .collect();
        track_points.sort_by(|a, b| {
            let first = a.x.cmp(&b.x);
            if first == std::cmp::Ordering::Equal {
                a.y.cmp(&b.y)
            } else {
                first
            }
        });
        track_points.dedup();
        if track_points.len() == 0 {
            Err(Box::from("No visible track points"))
        } else {
            Ok(ScreenActivity {
                name: self.name,
                date: self.date,
                track_points,
            })
        }
    }
}