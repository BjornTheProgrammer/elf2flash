use std::io::Stdout;

use elf2flash_core::ProgressReporter;
use pbr::{ProgressBar, Units};

pub struct ProgressBarReporter {
    pb: ProgressBar<Stdout>,
}

impl ProgressReporter for ProgressBarReporter {
    fn start(&mut self, total_bytes: usize) {
        self.pb.total = total_bytes as u64;
        self.pb.set_units(Units::Bytes);
    }

    fn advance(&mut self, bytes: usize) {
        self.pb.add(bytes as u64);
    }

    fn finish(&mut self) {
        self.pb.finish();
    }
}

impl ProgressBarReporter {
    pub fn new() -> Self {
        Self {
            pb: ProgressBar::new(0),
        }
    }
}
