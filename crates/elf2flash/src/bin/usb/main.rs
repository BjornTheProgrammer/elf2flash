use std::time::Duration;

use elf2flash_core::boards::{BoardIter, UsbDevice, UsbVersion};
use rusb::{
    Device, DeviceHandle, Direction, EndpointDescriptor, GlobalContext, InterfaceDescriptor,
    Language, TransferType, UsbContext,
};

use crate::usb_bulk::{BulkUsbChannel, send_scsi_inquiry};

mod usb_bulk;

#[allow(unused)]
struct OpenUsbDevice<T: UsbContext> {
    handle: DeviceHandle<T>,
    language: Language,
    timeout: Duration,
}

fn main() {
    let devices = rusb::devices().unwrap();
    let devices = devices
        .iter()
        .filter_map(|device| {
            let desc = device.device_descriptor().unwrap();
            let version = desc.device_version();

            let ret_device = UsbDevice {
                bus_number: device.bus_number(),
                address: device.address(),
                vendor_id: desc.vendor_id(),
                product_id: desc.product_id(),
                version: UsbVersion(version.0, version.1, version.2),
            };
            Some(ret_device)
        })
        .collect::<Vec<_>>();

    for device in devices {
        let board = match BoardIter::new().find(|board| board.is_device_board(&device)) {
            Some(board) => board,
            None => continue,
        };

        let timeout = Duration::from_secs(1);
        let mut usb_device = {
            match GlobalContext::default()
                .open_device_with_vid_pid(device.vendor_id, device.product_id)
            {
                Some(h) => match h.read_languages(timeout) {
                    Ok(l) => {
                        if !l.is_empty() {
                            OpenUsbDevice {
                                handle: h,
                                language: l[0],
                                timeout,
                            }
                        } else {
                            continue;
                        }
                    }
                    Err(_) => continue,
                },
                None => continue,
            }
        };

        let device = &usb_device.handle.device();

        let interface = get_mass_storage_interface(device, &mut usb_device).unwrap();

        println!("interface: {:?}", interface);

        let mut channel = BulkUsbChannel::new(
            usb_device.handle,
            interface.interface_number,
            interface.config_number,
            interface.in_address,
            interface.in_max_packet_size,
            interface.out_address,
            interface.out_max_packet_size,
            Duration::from_secs(1),
        )
        .unwrap();

        println!("board found: {:?}", board.family_id());
        println!("device: {:?}", device);

        // Add code to do a raw inquiery request via scsi here, only use rusb
        send_scsi_inquiry(&mut channel).unwrap();
    }
}

#[allow(unused)]
#[derive(Debug, Clone)]
struct MassStorageInterface {
    in_address: u8,
    in_max_packet_size: u16,
    out_address: u8,
    out_max_packet_size: u16,
    interface_number: u8,
    config_number: u8,
}
fn get_mass_storage_interface<'a>(
    device: &'a Device<GlobalContext>,
    usb_device: &mut OpenUsbDevice<GlobalContext>,
) -> Option<MassStorageInterface> {
    let description = device.device_descriptor().unwrap();

    for n in 0..description.num_configurations() {
        let config_desc = match device.config_descriptor(n) {
            Ok(desc) => desc,
            Err(_) => return None,
        };

        let interfaces = config_desc.interfaces();
        for interface in interfaces {
            for interface_desc in interface.descriptors() {
                if interface_desc.class_code() != 0x08 || interface_desc.sub_class_code() != 0x06 {
                    continue;
                }

                print_interface(&interface_desc, usb_device);

                let mut transfer_out_info = None;
                let mut transfer_in_info = None;

                let endpoints = interface_desc.endpoint_descriptors();
                for endpoint in endpoints {
                    if endpoint.transfer_type() != TransferType::Bulk {
                        continue;
                    }

                    print_endpoint(&endpoint);

                    match endpoint.direction() {
                        Direction::In => {
                            transfer_in_info =
                                Some((endpoint.address(), endpoint.max_packet_size()))
                        }
                        Direction::Out => {
                            transfer_out_info =
                                Some((endpoint.address(), endpoint.max_packet_size()))
                        }
                    }
                }

                if let Some(in_info) = transfer_in_info
                    && let Some(out_info) = transfer_out_info
                {
                    return Some(MassStorageInterface {
                        in_address: in_info.0,
                        in_max_packet_size: in_info.1,
                        out_address: out_info.0,
                        out_max_packet_size: out_info.1,
                        interface_number: interface_desc.interface_number(),
                        config_number: config_desc.number(),
                    });
                }
            }
        }
    }

    None
}

fn print_interface<T: UsbContext>(
    interface_desc: &InterfaceDescriptor,
    handle: &mut OpenUsbDevice<T>,
) {
    println!("    Interface Descriptor:");
    println!("      bLength              {:3}", interface_desc.length());
    println!(
        "      bDescriptorType      {:3}",
        interface_desc.descriptor_type()
    );
    println!(
        "      bInterfaceNumber     {:3}",
        interface_desc.interface_number()
    );
    println!(
        "      bAlternateSetting    {:3}",
        interface_desc.setting_number()
    );
    println!(
        "      bNumEndpoints        {:3}",
        interface_desc.num_endpoints()
    );
    println!(
        "      bInterfaceClass     {:#04x}",
        interface_desc.class_code()
    );
    println!(
        "      bInterfaceSubClass  {:#04x}",
        interface_desc.sub_class_code()
    );
    println!(
        "      bInterfaceProtocol  {:#04x}",
        interface_desc.protocol_code()
    );
    println!(
        "      iInterface           {:3} {}",
        interface_desc.description_string_index().unwrap_or(0),
        handle
            .handle
            .read_interface_string(handle.language, interface_desc, handle.timeout)
            .unwrap_or_default()
    );

    if interface_desc.extra().is_empty() {
        println!("    {:?}", interface_desc.extra());
    } else {
        println!("    no extra data");
    }
}

fn print_endpoint(endpoint_desc: &EndpointDescriptor) {
    println!("      Endpoint Descriptor:");
    println!("        bLength              {:3}", endpoint_desc.length());
    println!(
        "        bDescriptorType      {:3}",
        endpoint_desc.descriptor_type()
    );
    println!(
        "        bEndpointAddress    {:#04x} EP {} {:?}",
        endpoint_desc.address(),
        endpoint_desc.number(),
        endpoint_desc.direction()
    );
    println!("        bmAttributes:");
    println!(
        "          Transfer Type          {:?}",
        endpoint_desc.transfer_type()
    );
    println!(
        "          Synch Type             {:?}",
        endpoint_desc.sync_type()
    );
    println!(
        "          Usage Type             {:?}",
        endpoint_desc.usage_type()
    );
    println!(
        "        wMaxPacketSize    {:#06x}",
        endpoint_desc.max_packet_size()
    );
    println!(
        "        bInterval            {:3}",
        endpoint_desc.interval()
    );
}
