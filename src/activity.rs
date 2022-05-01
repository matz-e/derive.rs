use super::heat::Heatmap;

use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path;

use gpx::read;
use gpx::{Gpx, Track};
use geo::{Coordinate, Point};
use rayon::prelude::*;
use rayon::iter::IntoParallelRefIterator;

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
