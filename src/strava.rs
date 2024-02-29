use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use chrono::prelude::*;
use indicatif::ParallelProgressIterator;
use rayon::prelude::*;
use regex::Regex;

use super::activity::{RawActivity, ScreenActivity};
use super::heat::Heatmap;

pub struct DataExport {
    activities: Vec<RawActivity>,
}

type Record = HashMap<String, String>;

impl DataExport {
    pub fn new(path: &PathBuf) -> Result<Self, Box<dyn Error>> {
        let time_padding_re = Regex::new(r"(, )(\d:)")?;
        let date_padding_re = Regex::new(r"( )(\d,)")?;

        let mut no_files = 0;
        let mut read_errors = 0;
        let mut parse_errors = 0;

        let mut rdr = csv::Reader::from_path(path.join("activities.csv"))?;
        let activities: Vec<RawActivity> = rdr
            .deserialize()
            .filter_map(|result| {
                if result.is_err() {
                    read_errors += 1;
                    return None;
                }
                let record: Record = result.unwrap();
                let filename = &record["Filename"];
                if filename.is_empty() {
                    no_files += 1;
                    return None;
                }
                let raw_datetime = date_padding_re.replace(&record["Activity Date"], "${1}0${2}");
                let raw_datetime = time_padding_re.replace(&raw_datetime, "${1}0${2}");
                let raw_datetime = Utc.datetime_from_str(&raw_datetime, "%b %e, %Y, %r");
                if raw_datetime.is_err() {
                    parse_errors += 1;
                    return None;
                }
                let datetime = raw_datetime.unwrap();
                Some(RawActivity::new(
                    record["Activity Name"].clone(),
                    datetime,
                    path.join(filename),
                ))
            })
            .collect();
        if no_files > 0 {
            eprintln!("Found {} activities without files", no_files);
        }
        if read_errors > 0 {
            eprintln!("Could not read {} activity records", read_errors);
        }
        if parse_errors > 0 {
            eprintln!("Could not parse {} timestamps", parse_errors);
        }
        Ok(DataExport { activities })
    }

    pub fn parse(&self, map: &mut Heatmap) -> Vec<ScreenActivity> {
        let n = self.activities.len();
        eprint!("Parsing {:?} files", n);

        let mut activities: Vec<ScreenActivity> = self
            .activities
            .par_iter()
            .progress_count(n as u64)
            .filter_map(|a| a.parse().ok())
            .filter_map(|a| a.project_to_screen(&map).ok())
            .collect();
        activities.sort_by_key(|a| a.date);
        activities
    }
}
