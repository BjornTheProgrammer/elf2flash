use fatfs::{FileSystem, FsOptions};
use usbh_fatfs::{FatPartition, PartitionView, StorageUsb};

fn main() {
    env_logger::init();

    let usbs = StorageUsb::list_usbs().unwrap();
    for mut usb in usbs {
        let partitions = FatPartition::list_partitions(&mut usb).unwrap();
        for partition in partitions {
            let mut block_device = usb.open().unwrap().block_device().unwrap();
            let part_view = PartitionView {
                inner: &mut block_device,
                start: partition.first_byte as u64,
                len: partition.length as u64,
            };
            let fatfs = FileSystem::new(part_view, FsOptions::new()).unwrap();
        }
    }
}
