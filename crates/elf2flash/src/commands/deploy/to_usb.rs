use std::io::Write;

use anyhow::{Result, bail};
use elf2flash_core::{
    ProgressReporter,
    boards::{BoardInfo, BoardIter, UsbDevice, UsbVersion},
};
use fatfs::{FileSystem, FsOptions};
use usbh_fatfs::{FatPartition, PartitionView, StorageUsb};

pub fn get_plugged_in_boards() -> Result<Vec<(UsbDevice, Option<Box<dyn BoardInfo>>, StorageUsb)>> {
    let mut boards_found = Vec::new();

    for usb in StorageUsb::list_usbs()? {
        let desc = match usb.usb_device.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };
        let version = desc.device_version();

        let usb_device = UsbDevice {
            bus_number: usb.usb_device.bus_number(),
            address: usb.usb_device.address(),
            vendor_id: desc.vendor_id(),
            product_id: desc.product_id(),
            version: UsbVersion(version.0, version.1, version.2),
        };

        if let Some(board) = BoardIter::new().find(|b| b.is_device_board(&usb_device)) {
            boards_found.push((usb_device, Some(board), usb));
        }
    }

    if boards_found.is_empty() {
        log::warn!("No recognized boards found, falling back to generic UF2 devices");

        for usb in StorageUsb::list_usbs()? {
            let desc = match usb.usb_device.device_descriptor() {
                Ok(d) => d,
                Err(_) => continue,
            };

            let version = desc.device_version();
            let usb_device = UsbDevice {
                bus_number: usb.usb_device.bus_number(),
                address: usb.usb_device.address(),
                vendor_id: desc.vendor_id(),
                product_id: desc.product_id(),
                version: UsbVersion(version.0, version.1, version.2),
            };

            boards_found.push((usb_device, None, usb));
        }
    }

    Ok(boards_found)
}

pub fn list_uf2_partitions(
    board: &dyn BoardInfo,
    storage_usb: &mut StorageUsb,
) -> Result<Vec<FatPartition>> {
    let mut uf2_partitions = Vec::new();
    let partitions = match FatPartition::list_partitions(storage_usb) {
        Ok(part) => part,
        Err(err) => {
            log::warn!(
                "Failed to list partitions for board '{}' (family id {:#x}): {err:?}",
                board.board_name(),
                board.family_id()
            );

            bail!(
                "Failed to list partitions for board '{}' (family id {:#x}): {err:?}",
                board.board_name(),
                board.family_id()
            )
        }
    };
    for partition in partitions {
        let opened = match storage_usb.open() {
            Ok(opened) => opened,
            Err(err) => {
                log::error!(
                    "Failed to open USB mass storage for board '{}' (family id {:#x}): {err:?}",
                    board.board_name(),
                    board.family_id()
                );
                continue;
            }
        };
        let mut block_device = match opened.block_device() {
            Ok(dev) => dev,
            Err(err) => {
                log::error!(
                    "Failed to get block device for board '{}' (family id {:#x}): {err:?}",
                    board.board_name(),
                    board.family_id()
                );
                continue;
            }
        };

        let part_view = PartitionView {
            inner: &mut block_device,
            start: partition.first_byte as u64,
            len: partition.length as u64,
        };

        let fatfs = match FileSystem::new(part_view, FsOptions::new()) {
            Ok(fs) => fs,
            Err(err) => {
                log::error!(
                    "Failed to mount FAT filesystem on board '{}' (family id {:#x}): {err:?}",
                    board.board_name(),
                    board.family_id()
                );
                continue;
            }
        };
        let mut contains_info_uf2 = false;
        for item in fatfs.root_dir().iter() {
            let item = match item {
                Ok(item) => item,
                Err(err) => {
                    log::debug!(
                        "Failed to read item on FAT filesystem on board '{}' (family id {:#x}): {err:?}",
                        board.board_name(),
                        board.family_id()
                    );
                    continue;
                }
            };
            let name = item.file_name();
            if name.contains("INFO_UF2.TXT") {
                contains_info_uf2 = true;
            }
        }

        if !contains_info_uf2 {
            log::debug!(
                "Partition on board '{}' does not contain INFO_UF2.TXT, skipping",
                board.board_name()
            );
            continue;
        }

        log::debug!(
            "Found partition on board '{}' that contains INFO_UF2.TXT",
            board.board_name()
        );

        uf2_partitions.push(partition);
    }

    Ok(uf2_partitions)
}

pub fn deploy_to_usb<B: AsRef<[u8]>>(
    out_file: B,
    partition: &FatPartition,
    board: &dyn BoardInfo,
    storage_usb: &mut StorageUsb,
    mut progress: impl ProgressReporter,
) -> anyhow::Result<()> {
    progress.start(out_file.as_ref().len());

    log::info!(
        "Writing firmware to board '{}' (family id {:#x})",
        board.board_name(),
        board.family_id()
    );

    let opened = match storage_usb.open() {
        Ok(opened) => opened,
        Err(err) => {
            log::error!(
                "Failed to open USB mass storage for board '{}' (family id {:#x}): {err:?}",
                board.board_name(),
                board.family_id()
            );
            bail!(
                "Failed to open USB mass storage for board '{}' (family id {:#x}): {err:?}",
                board.board_name(),
                board.family_id()
            );
        }
    };

    let mut block_device = match opened.block_device() {
        Ok(dev) => dev,
        Err(err) => {
            log::error!(
                "Failed to get block device for board '{}' (family id {:#x}): {err:?}",
                board.board_name(),
                board.family_id()
            );
            bail!(
                "Failed to get block device for board '{}' (family id {:#x}): {err:?}",
                board.board_name(),
                board.family_id()
            );
        }
    };

    let part_view = PartitionView {
        inner: &mut block_device,
        start: partition.first_byte as u64,
        len: partition.length as u64,
    };

    let fatfs = match FileSystem::new(part_view, FsOptions::new()) {
        Ok(fs) => fs,
        Err(err) => {
            bail!(
                "Failed to mount FAT filesystem on board '{}' (family id {:#x}): {err:?}",
                board.board_name(),
                board.family_id()
            );
        }
    };

    match fatfs.root_dir().create_file("out.uf2") {
        Ok(mut file) => {
            const CHUNK_SIZE: usize = 16 * 1024; // tune this

            for chunk in out_file.as_ref().chunks(CHUNK_SIZE) {
                match file.write_all(chunk) {
                    Ok(_) => (),
                    Err(err) => log::error!(
                        "Failed to write out.uf2 to board '{}': {err:?}",
                        board.board_name()
                    ),
                }
                progress.advance(chunk.len()); // only once per chunk
            }

            if let Err(err) = file.flush() {
                log::error!(
                    "Failed to flush out.uf2 to board '{}': {err:?}",
                    board.board_name()
                );
            }
            progress.finish();
        }
        Err(err) => {
            log::error!(
                "Failed to create out.uf2 on board '{}': {err:?}",
                board.board_name()
            );
        }
    }

    Ok(())
}
