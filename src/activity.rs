use super::heat::Heatmap;

use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use fitparser::profile::field_types;
use flate2::read::GzDecoder;
use geo::Point;
use geo_types::Coord;
use gpx::{Gpx, Track};
use time::OffsetDateTime;

fn extract_coordinate(field: &fitparser::FitDataField) -> Option<f64> {
    if field.units() == "semicircles" {
        if let fitparser::Value::SInt32(raw) = field.value() {
            return Some(*raw as f64 * 180.0 / u32::pow(2, 31) as f64);
        }
    }
    None
}

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
                    lat = extract_coordinate(field);
                } else if field.name() == "position_long" {
                    lon = extract_coordinate(field);
                }
            }
            if let Some((x, y)) = lon.zip(lat) {
                activity.track_points.push(Point::new(x, y));
            }
        }
    }

    if activity.track_points.is_empty() {
        Err(Box::from("No track points"))
    } else {
        Ok(activity)
    }
}

fn parse_gpx<T: std::io::Read>(reader: &mut BufReader<T>) -> Result<Activity, Box<dyn Error>> {
    let gpx: Gpx = gpx::read(reader)?;
    // Nothing to do if there are no tracks
    if gpx.tracks.is_empty() {
        return Err(Box::from("file has no tracks"));
    } else if gpx.tracks.len() > 1 {
        eprintln!("Warning! more than 1 track, just taking first");
    }

    let track: &Track = &gpx.tracks[0];

    let mut activity = Activity {
        name: track
            .name
            .clone()
            .unwrap_or_else(|| String::from("Untitled")),
        date: chrono::Utc::now(),
        track_points: vec![],
    };

    if let Some(metadata) = gpx.metadata {
        if let Some(time) = metadata.time {
            activity.date = chrono::DateTime::from_timestamp(
                OffsetDateTime::from(time).unix_timestamp(), 0
            ).expect("Timestamp conversion failed");
        }
    }

    // Append all the waypoints.
    for seg in track.segments.iter() {
        let points = seg.points.iter().map(|wpt| wpt.point());
        activity.track_points.extend(points);
    }

    if activity.track_points.is_empty() {
        Err(Box::from("No track points"))
    } else {
        Ok(activity)
    }
}

fn parse<T: std::io::Read>(
    reader: &mut BufReader<T>,
    path: &Path,
) -> Result<Activity, Box<dyn Error>> {
    if path.extension() == Some(OsStr::new("gpx")) {
        parse_gpx(reader)
    } else if path.extension() == Some(OsStr::new("fit")) {
        parse_fit(reader)
    } else {
        Err(Box::from("Unknown file type"))
    }
}

pub struct RawActivity {
    name: String,
    date: chrono::DateTime<chrono::Utc>,
    path: PathBuf,
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
    pub track_points: Vec<Coord<u32>>,
}

impl RawActivity {
    pub fn new(name: String, date: chrono::DateTime<chrono::Utc>, path: PathBuf) -> Self {
        RawActivity { name, date, path }
    }

    pub fn parse(self) -> Result<Activity, Box<dyn Error>> {
        let file = File::open(&self.path)?;
        let mut activity = if self.path.extension() == Some(OsStr::new("gz")) {
            let decoder = GzDecoder::new(file);
            let mut reader = BufReader::new(decoder);
            parse(&mut reader, &self.path.with_extension(""))
        } else {
            let mut reader = BufReader::new(file);
            parse(&mut reader, &self.path)
        }?;
        activity.name = self.name;
        activity.date = self.date;
        Ok(activity)
    }
}

impl Activity {
    pub fn project_to_screen(self, heatmap: &dyn Heatmap) -> Result<ScreenActivity, Box<dyn Error>> {
        let mut track_points: Vec<Coord<u32>> = self
            .track_points
            .iter()
            .filter_map(|pt| heatmap.project_to_screen(pt))
            .collect();
        track_points.dedup();
        if track_points.is_empty() {
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
