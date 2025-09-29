use std::io::Write;

use elf2flash_core::boards::{BoardIter, UsbDevice, UsbVersion};
use fatfs::{FileSystem, FsOptions};
use usbh_fatfs::{FatPartition, PartitionView, StorageUsb};

fn main() {
    env_logger::init();

    let usbs = StorageUsb::list_usbs().unwrap();
    for mut usb in usbs {
        let device_descriptor = usb.usb_device.device_descriptor().unwrap();
        let version = device_descriptor.device_version();

        let usb_device = UsbDevice {
            bus_number: usb.usb_device.bus_number(),
            address: usb.usb_device.address(),
            vendor_id: device_descriptor.vendor_id(),
            product_id: device_descriptor.product_id(),
            version: UsbVersion(version.0, version.1, version.2),
        };

        let board = match BoardIter::new().into_iter().find(|board| board.is_device_board(&usb_device)) {
            Some(board) => board,
            None => continue,
        };

        let partitions = FatPartition::list_partitions(&mut usb).unwrap();
        for partition in partitions {
            let mut block_device = usb.open().unwrap().block_device().unwrap();
            let part_view = PartitionView {
                inner: &mut block_device,
                start: partition.first_byte as u64,
                len: partition.length as u64,
            };
            let fatfs = FileSystem::new(part_view, FsOptions::new()).unwrap();
            let mut contains_info_uf2 = false;
            for dir in fatfs.root_dir().iter() {
                let name = dir.unwrap().file_name();
                if name.contains("INFO_UF2.TXT") {
                    contains_info_uf2 = true;
                }
            }

            if !contains_info_uf2 { continue; }

            println!("going to write!");

            let mut file = fatfs.root_dir().create_file("out.uf2").unwrap();
            file.write_all(include_bytes!("/home/bjorn/Documents/GitHub/DevilArm/devil-embedded/out.uf2")).unwrap();
            file.flush().unwrap();
        }
    }
}
