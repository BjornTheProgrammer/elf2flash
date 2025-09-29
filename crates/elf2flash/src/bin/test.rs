use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::MetadataExt,
};

use rusb::Device;
use sysinfo::Disks;
fn find_mount_source(target: &str) -> Option<String> {
    let disks = Disks::new_with_refreshed_list();
    for disk in &disks {
        println!("disk: {:?}", disk.name());
        let mount_point = disk.mount_point().to_string_lossy();
        if mount_point == target {
            return Some(disk.name().to_string_lossy().into_owned());
        }
    }
    None
}

fn main() {
    let meta = fs::metadata("/media/bjorn/RP2350").unwrap();
    println!("meta: {:#x?}", meta.dev());

    if let Some(src) = find_mount_source("/media/bjorn/RP2350") {
        println!("Mounted source: {}", src);
    } else {
        println!("Not found");
    }
}
