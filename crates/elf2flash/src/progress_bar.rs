use std::io::Stdout;

use elf2flash_core::ProgressReporter;
use log::{LevelFilter, max_level};
use pbr::{ProgressBar, Units};

pub struct ProgressBarReporter {
    pb: Option<ProgressBar<Stdout>>,
}

impl ProgressReporter for ProgressBarReporter {
    fn start(&mut self, total_bytes: usize) {
        if let Some(pb) = self.pb.as_mut() {
            pb.total = total_bytes as u64;
            pb.set_units(Units::Bytes);
        }
    }

    fn advance(&mut self, bytes: usize) {
        if let Some(pb) = self.pb.as_mut() {
            pb.add(bytes as u64);
        }
    }

    fn finish(&mut self) {
        if let Some(pb) = self.pb.as_mut() {
            pb.finish();
        }
    }
}

impl ProgressBarReporter {
    pub fn new() -> Self {
        let should_log = if max_level() >= LevelFilter::Info {
            true
        } else {
            false
        };

        if should_log {
            Self {
                pb: Some(ProgressBar::new(0)),
            }
        } else {
            Self { pb: None }
        }
    }
}
