use usbh_fatfs::{
    FatPartition, PartitionView, StorageUsb,
    fatfs::{FileSystem, FsOptions},
};

fn main() {
    let usbs = StorageUsb::list_usbs().unwrap();
    println!("usbs: {:?}", usbs);
    // usbs: [StorageUsb { inner: Closed(UsbMassStorage { device: Bus 003 Device 016: ID 2e8a:000f, device_config_number: 1, extra: Closed }), usb_device: Bus 003 Device 016: ID 2e8a:000f }]

    for mut usb in usbs {
        let partitions = FatPartition::list_partitions(&mut usb).unwrap();
        println!("\n\npartitions: {:?}", partitions);
        // partitions: [FatPartition { inner: Partition { id: 0, first_byte: 512, len: 134217216, attributes: MBR { bootable: false, type_code: 14 } }, volume_id: 3802154214, volume_label: "RP2350", fat_type: Fat16, cluster_size: 4096, first_byte: 512, length: 134217216 }]

        for partition in partitions {
            let opened = usb.open().unwrap();
            let mut block_device = opened.block_device().unwrap();

            let part_view =
                PartitionView::new(&mut block_device, partition.first_byte, partition.length)
                    .unwrap();

            let fatfs = FileSystem::new(part_view, FsOptions::new()).unwrap();
            println!("\nFound fatfs filesystem");
            println!("    label: {}", fatfs.volume_label());
            println!("    id: {}", fatfs.volume_id());
            // Found fatfs filesystem
            //     label: RP2350
            //     id: 3802154214

            println!("\nListing root dir on volume");
            for item in fatfs.root_dir().iter() {
                let item = item.unwrap();
                println!("    {}", item.file_name());
            }

            // Listing root dir on volume
            //     INDEX.HTM
            //     INFO_UF2.TXT
        }
    }
}
