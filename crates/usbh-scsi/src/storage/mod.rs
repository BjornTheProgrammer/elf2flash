//! USB Mass Storage transport layer.
//!
//! This module provides the building blocks to enumerate, open, and
//! communicate with USB Mass Storage devices using the Bulk-Only
//! Transport (BOT) protocol, as defined in the USB Mass Storage Class
//! specification.
//!
//! # Design
//!
//! - [`UsbMassStorage`] is the central abstraction. It is
//!   parameterized by state:
//!   - [`Closed`]: device is enumerated but not opened.
//!   - [`Opened`]: device is claimed, transport endpoints are active,
//!     and I/O is possible.
//!
//! - [`UsbMassStorage::list`] scans all connected USB devices and
//!   filters those that expose a Mass Storage interface (class code
//!   `0x08`). Results are returned in the `Closed` state.
//!
//! - A `Closed` device can be transitioned to [`Opened`] by calling
//!   [`UsbMassStorage::open`]. This will:
//!   - Claim the Mass Storage interface.
//!   - Identify bulk IN/OUT endpoints.
//!   - Prepare a [`BulkOnlyTransport`] handle.
//!
//! - Once `Opened`, raw bulk I/O is available via [`write`] and
//!   [`read`]. Higher-level SCSI commands can be executed with
//!   [`execute_command`].
//!
//! - For block-level abstraction, [`UsbMassStorage::block_device`]
//!   constructs a [`UsbBlockDevice`], which implements `Read`, `Write`,
//!   and `Seek` for random-access sector I/O.
//!
//! # Example
//!
//! ```
//! use usbh_scsi::storage::UsbMassStorage;
//! use usbh_scsi::commands::inquiry::InquiryCommand;
//! use usbh_scsi::commands::cbw::Direction;
//! use std::error::Error;
//!
//! fn main() -> Result<(), Box<dyn Error>> {
//!     // Enumerate all connected MSC devices
//!     let mut devices = UsbMassStorage::list()?;
//!     if let Some(closed) = devices.pop() {
//!         // Open the first one
//!         let mut dev = closed.open()?;
//!
//!         // Send a SCSI INQUIRY command
//!         let cmd = InquiryCommand::new(0);
//!         let mut buf = [0u8; 36];
//!         dev.execute_command(1, buf.len() as u32, Direction::In, &cmd, Some(&mut buf))?;
//!
//!         println!("INQUIRY data: {:?}", &buf);
//!
//!         // Close again when done
//!         let _closed = dev.close();
//!     }
//!     Ok(())
//! }
//! ```
//!
//! # Notes
//!
//! - Only Bulk-Only Transport (protocol code `0x50`) is supported, if you want other transport methods, create an issue, I'll be happy to implement it.
//! - `GET_MAX_LUN` is provided via [`UsbMassStorage::get_max_lun`],
//!   though most devices report only `0`.
//! - All timeouts default to 10 seconds but may be tuned via the
//!   [`Opened`] state.
//!
//! [`write`]: UsbMassStorage::write
//! [`read`]: UsbMassStorage::read
//! [`execute_command`]: UsbMassStorage::execute_command

use rusb::{ConfigDescriptor, Device, DeviceHandle, Direction, GlobalContext, TransferType};
use thiserror::Error;

use crate::{
    commands::{self, CommandBlock, cbw::Cbw},
    storage::block_device::UsbBlockDevice,
};

pub mod block_device;

/// Errors that can occur while enumerating or opening USB Mass Storage devices.
#[derive(Error, Debug)]
pub enum UsbMassStorageError {
    /// Failed to retrieve device list from rusb.
    #[error("failed to get usb devices from rusb")]
    FailedToGetUsbDevices,
    /// Failed to open a selected device.
    #[error("failed to open usb devices from rusb")]
    FailedToOpenUsbDevice,
    /// Failed to claim interface.
    #[error("failed to claim interface for usb devices from rusb `{0}`")]
    FailedToClaimInterfaceFromUsbDevice(rusb::Error),
}

/// A USB Mass Storage device, parameterized by its state (`Closed` or `Opened`).
///
/// - In `Closed` state, the device is enumerated but not opened.
/// - In `Opened` state, the device is claimed and ready for I/O.
#[derive(Debug, Clone)]
pub struct UsbMassStorage<S = Closed> {
    pub device: Device<GlobalContext>,
    pub device_config_number: u8,
    pub extra: S,
}

/// State for an opened USB Mass Storage device.
///
/// Holds the active `DeviceHandle`, transport information,
/// and a default timeout duration.
#[derive(Debug)]
pub struct Opened {
    pub handle: DeviceHandle<GlobalContext>,
    pub bulk_only_transport: Option<BulkOnlyTransport>,
    pub timeout_duration: core::time::Duration,
}

/// Marker type representing a closed USB Mass Storage device.
#[derive(Debug, Clone)]
pub struct Closed;

/// USB Bulk-Only Transport (BOT) information for a Mass Storage interface.
///
/// Includes endpoint addresses, max packet sizes, and interface number.
#[derive(Debug)]
pub struct BulkOnlyTransport {
    pub in_address: u8,
    pub in_max_size: u16,
    pub out_address: u8,
    pub out_max_size: u16,
    pub interface_number: u8,
}

impl UsbMassStorage<Closed> {
    /// Attempt to open the device and transition it into the [`Opened`] state.
    ///
    /// - Claims the MSC interface.
    /// - Locates IN/OUT bulk endpoints.
    /// - Configures the active configuration and alternate setting.
    pub fn open(self) -> Result<UsbMassStorage<Opened>, UsbMassStorageError> {
        let handle = match self
            .device
            .open() {
                Ok(val) => val,
                Err(err) => {
                    match err {
                        rusb::Error::Access => {
                            log::error!("Insufficient permissions to open usb device");
                        },
                         _ => (),
                    }

                    return Err(UsbMassStorageError::FailedToOpenUsbDevice)
                },
            };

        handle.set_auto_detach_kernel_driver(true).ok();

        handle
            .set_active_configuration(self.device_config_number)
            .ok();

        let mut bulk_only_transport = None;

        let config = self
            .device
            .config_descriptor_by_number(self.device_config_number)
            .map_err(|_| UsbMassStorageError::FailedToOpenUsbDevice)?
            .ok_or(UsbMassStorageError::FailedToOpenUsbDevice)?;
        for interface in config.interfaces() {
            for interface_descriptor in interface.descriptors() {
                // Check if class is not mass storage interface or not a SCSI transparent command set
                if interface_descriptor.class_code() != 0x08
                    || interface_descriptor.sub_class_code() != 0x06
                {
                    continue;
                }

                // Check if not just a USB Mass Storage Class Bulk-Only (BBB) Transport
                if interface_descriptor.protocol_code() != 0x50 {
                    continue;
                }

                let mut transfer_out_info = None;
                let mut transfer_in_info = None;

                let endpoints = interface_descriptor.endpoint_descriptors();
                for endpoint in endpoints {
                    if endpoint.transfer_type() != TransferType::Bulk {
                        continue;
                    }

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
                    bulk_only_transport = Some(BulkOnlyTransport {
                        in_address: in_info.0,
                        in_max_size: in_info.1,
                        out_address: out_info.0,
                        out_max_size: out_info.1,
                        interface_number: interface_descriptor.interface_number(),
                    });
                }
            }
        }

        if let Some(bulk_only_transport) = &bulk_only_transport {
            match handle
                .claim_interface(bulk_only_transport.interface_number) {
                    Ok(_) => (),
                    Err(err) => {
                        if err == rusb::Error::NotSupported {
                            log::error!("Interface not supported on device. If using windows, installing a usb driver, like Zadig (https://zadig.akeo.ie/), will likely solve the issue.");
                            return Err(UsbMassStorageError::FailedToClaimInterfaceFromUsbDevice(rusb::Error::NotSupported))
                        } else {
                            return Err(UsbMassStorageError::FailedToClaimInterfaceFromUsbDevice(err))
                        }
                    },
                }
                
            handle
                .set_alternate_setting(bulk_only_transport.interface_number, 0)
                .ok();

            handle.clear_halt(bulk_only_transport.in_address).ok();
            handle.clear_halt(bulk_only_transport.out_address).ok();
        }

        Ok(UsbMassStorage::<Opened> {
            device: self.device,
            device_config_number: self.device_config_number,
            extra: Opened {
                handle,
                bulk_only_transport,
                timeout_duration: core::time::Duration::from_secs(10),
            },
        })
    }
}

impl UsbMassStorage<Opened> {
    /// Close the device, releasing any claimed interfaces.
    pub fn close(self) -> UsbMassStorage<Closed> {
        UsbMassStorage::<Closed> {
            device: self.device,
            device_config_number: self.device_config_number,
            extra: Closed,
        }
    }

    /// Write raw bytes to the bulk OUT endpoint.
    ///
    /// Returns the number of bytes successfully sent.
    pub fn write<B: AsRef<[u8]>>(
        &mut self,
        bytes: B,
    ) -> Result<usize, UsbMassStorageReadWriteError> {
        let bulk_only_transport = match self.extra.bulk_only_transport {
            Some(ref bulk) => bulk,
            None => return Err(UsbMassStorageReadWriteError::NoKnownTransportationMethod),
        };

        let data = bytes.as_ref();
        let n = self.extra.handle.write_bulk(
            bulk_only_transport.out_address,
            data,
            self.extra.timeout_duration,
        )?;
        Ok(n)
    }

    /// Read raw bytes from the bulk IN endpoint.
    ///
    /// Fills the provided buffer and returns the number of bytes read.
    pub fn read<B: AsMut<[u8]>>(
        &self,
        mut buffer: B,
    ) -> Result<usize, UsbMassStorageReadWriteError> {
        let bulk_only_transport = match self.extra.bulk_only_transport {
            Some(ref bulk) => bulk,
            None => return Err(UsbMassStorageReadWriteError::NoKnownTransportationMethod),
        };

        let buf = buffer.as_mut();
        let n = self.extra.handle.read_bulk(
            bulk_only_transport.in_address,
            buf,
            self.extra.timeout_duration,
        )?;
        Ok(n)
    }

    /// Execute a SCSI command using the Bulk-Only Transport protocol.
    ///
    /// - Sends a Command Block Wrapper (CBW).
    /// - Performs the data phase (if any).
    /// - Reads and validates the Command Status Wrapper (CSW).
    pub fn execute_command<T: CommandBlock>(
        &mut self,
        tag: u32,
        data_len: u32,
        direction: commands::cbw::Direction,
        cmd: &T,
        data_buf: Option<&mut [u8]>,
    ) -> Result<(), UsbMassStorageReadWriteError> {
        // 1. Send CBW
        let cbw = Cbw::new(tag, data_len, direction, cmd);
        self.write(cbw.to_bytes())?;

        // 2. Data phase
        if let Some(buf) = data_buf {
            match direction {
                commands::cbw::Direction::In => {
                    let len = buf.len();
                    let n = self.read(buf)?;
                    assert_eq!(n, len);
                }
                commands::cbw::Direction::Out => {
                    self.write(buf)?;
                }
            }
        }

        // 3. Read CSW (13 bytes)
        let mut csw = [0u8; 13];
        let n = self.read(&mut csw)?;
        assert_eq!(n, 13, "short CSW");

        Ok(())
    }

    /// Perform the `GET_MAX_LUN` class-specific request.
    ///
    /// Returns the highest supported Logical Unit Number.
    /// Most devices return `0` (only LUN0).
    pub fn get_max_lun(&mut self) -> Result<u8, UsbMassStorageReadWriteError> {
        let bulk_only_transport = match self.extra.bulk_only_transport {
            Some(ref bulk) => bulk,
            None => return Err(UsbMassStorageReadWriteError::NoKnownTransportationMethod),
        };

        let bm_request_type = rusb::request_type(
            rusb::Direction::In,
            rusb::RequestType::Class,
            rusb::Recipient::Interface,
        );
        let b_request = 0xFE; // GET_MAX_LUN
        let w_value = 0;
        let w_index = bulk_only_transport.interface_number as u16;
        let mut buf = [0u8; 1];

        match self.extra.handle.read_control(
            bm_request_type,
            b_request,
            w_value,
            w_index,
            &mut buf,
            self.extra.timeout_duration,
        ) {
            Ok(1) => Ok(buf[0]),
            Ok(_) => Ok(0), // if unexpected size, fallback to 0
            Err(rusb::Error::Pipe) => {
                // Devices with only one LUN often STALL this request → treat as 0
                Ok(0)
            }
            Err(e) => Err(UsbMassStorageReadWriteError::UsbDeviceBulkFailed(e)),
        }
    }

    /// Create a [`UsbBlockDevice`] abstraction for block-level I/O.
    pub fn block_device<'a>(&'a mut self) -> std::io::Result<UsbBlockDevice<'a>> {
        UsbBlockDevice::new(self)
    }
}

/// Errors that can occur during bulk I/O or SCSI execution.
#[derive(Error, Debug)]
pub enum UsbMassStorageReadWriteError {
    /// No suitable transport (Bulk-Only Transport) was found for this device.
    #[error("there is no defined transportation method")]
    NoKnownTransportationMethod,
    /// Low-level bulk transfer failed.
    #[error("bulk read error")]
    UsbDeviceBulkFailed(#[from] rusb::Error),
}

impl Drop for Opened {
    /// Resets the handle and releases the claimed interface on drop.
    fn drop(&mut self) {
        let _ = self.handle.reset();
        if let Some(bulk_only_transport) = &self.bulk_only_transport {
            let _ = self
                .handle
                .release_interface(bulk_only_transport.interface_number);
        }
    }
}

impl UsbMassStorage {
    /// Enumerate all connected USB Mass Storage devices.
    ///
    /// Filters by class code `0x08` (MSC). Returns devices in the `Closed` state.
    pub fn list() -> Result<Vec<UsbMassStorage<Closed>>, UsbMassStorageError> {
        let mut devices = Vec::new();
        let rusb_devices =
            rusb::devices().map_err(|_| UsbMassStorageError::FailedToGetUsbDevices)?;

        for device in rusb_devices.iter() {
            let desc = match device.device_descriptor() {
                Ok(desc) => desc,
                Err(_) => continue,
            };

            'configs: for i in 0..desc.num_configurations() {
                let config_desc = match device.config_descriptor(i) {
                    Ok(config) => config,
                    Err(_) => continue,
                };

                for interface in config_desc.interfaces() {
                    for interface_desc in interface.descriptors() {
                        if interface_desc.class_code() != 0x08 {
                            continue;
                        }
                        devices.push(UsbMassStorage {
                            device: device.clone(),
                            device_config_number: config_desc.number(),
                            extra: Closed,
                        });
                        break 'configs;
                    }
                }
            }
        }

        Ok(devices)
    }
}

/// Extension trait to fetch a configuration descriptor by number.
pub trait ConfigDescriptorExt {
    fn config_descriptor_by_number(&self, number: u8) -> rusb::Result<Option<ConfigDescriptor>>;
}

impl ConfigDescriptorExt for Device<GlobalContext> {
    fn config_descriptor_by_number(&self, number: u8) -> rusb::Result<Option<ConfigDescriptor>> {
        let desc = self.device_descriptor()?;
        for idx in 0..desc.num_configurations() {
            let config = self.config_descriptor(idx)?;
            if config.number() == number {
                return Ok(Some(config));
            }
        }
        Ok(None)
    }
}
