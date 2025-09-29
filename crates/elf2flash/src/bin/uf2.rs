use rusb::{Context, DeviceHandle, GlobalContext, UsbContext};
use std::io::{self};
use std::time::Duration;

// ---------------- HF2 constants (adjust if your uf2hid.h differs) ----------------
const EP_SIZE: usize = 64; // HID report payload (not counting report-id byte)
const REPORT_LEN: usize = EP_SIZE + 1; // +1 for report-id (we send 0)

const HF2_FLAG_CMDPKT_BODY: u8 = 0x00;
const HF2_FLAG_CMDPKT_LAST: u8 = 0x40;
const HF2_FLAG_SERIAL_OUT: u8 = 0x80;
const HF2_FLAG_SERIAL_ERR: u8 = 0xC0;
const HF2_FLAG_MASK: u8 = 0xC0;
const HF2_SIZE_MASK: u8 = 0x3F;

const HF2_CMD_INFO: u32 = 0x0001;
const HF2_CMD_BININFO: u32 = 0x0002;
const HF2_CMD_START_FLASH: u32 = 0x0003;
const HF2_CMD_WRITE_FLASH_PAGE: u32 = 0x0004;
const HF2_CMD_CHKSUM_PAGES: u32 = 0x0005;
const HF2_CMD_RESET_INTO_APP: u32 = 0x0008;
// BININFO response layout we care about:
const HF2_MODE_BOOTLOADER: u8 = 0x01;

// ---------------- small helpers ----------------
fn w16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
fn w32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
fn r16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}
fn r32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

// Find interface with an interrupt IN and OUT pair (typical HID alt 0).
fn find_hf2_endpoints<T: UsbContext>(
    handle: &DeviceHandle<T>,
) -> rusb::Result<(u8 /*iface*/, u8 /*ep_in*/, u8 /*ep_out*/)> {
    let dev = handle.device();
    let cfg = dev.active_config_descriptor()?;

    for iface in cfg.interfaces() {
        for desc in iface.descriptors() {
            let mut ep_in = None;
            let mut ep_out = None;
            for ep in desc.endpoint_descriptors() {
                let addr = ep.address();
                let is_interrupt = ep.transfer_type() == rusb::TransferType::Interrupt;
                if !is_interrupt {
                    continue;
                }
                if addr & 0x80 != 0 {
                    ep_in = Some(addr);
                } else {
                    ep_out = Some(addr);
                }
            }
            if let (Some(i), Some(o)) = (ep_in, ep_out) {
                return Ok((iface.number(), i, o));
            }
        }
    }
    Err(rusb::Error::NotFound)
}

// HF2 write: splits into report-sized chunks with tag byte.
fn send_hid<T: UsbContext>(
    handle: &DeviceHandle<T>,
    ep_out: u8,
    payload: &[u8],
    timeout: Duration,
) -> rusb::Result<()> {
    let mut remaining = payload;
    while !remaining.is_empty() {
        let chunk = remaining.len().min(EP_SIZE - 1); // -1 for tag
        let tag = if chunk == remaining.len() {
            HF2_FLAG_CMDPKT_LAST
        } else {
            HF2_FLAG_CMDPKT_BODY
        };
        let mut report = [0u8; REPORT_LEN];
        // report[0] = 0 (report id)
        report[1] = tag | (chunk as u8 & HF2_SIZE_MASK);
        report[2..2 + chunk].copy_from_slice(&remaining[..chunk]);
        let wrote = handle.write_interrupt(ep_out, &report, timeout)?;
        if wrote != REPORT_LEN {
            return Err(rusb::Error::Other);
        }
        remaining = &remaining[chunk..];
    }
    Ok(())
}

// HF2 read: accumulates body chunks until tag != BODY; also handles serial packets (ignored here).
fn recv_hid<T: UsbContext>(
    handle: &DeviceHandle<T>,
    ep_in: u8,
    timeout: Duration,
    buf_accum: &mut Vec<u8>,
) -> rusb::Result<()> {
    buf_accum.clear();
    let mut report = [0u8; REPORT_LEN];
    loop {
        let n = handle.read_interrupt(ep_in, &mut report, timeout)?;
        if n != REPORT_LEN {
            return Err(rusb::Error::Other);
        }
        // skip report[0] (report id)
        let tag = report[1];
        let size = (tag & HF2_SIZE_MASK) as usize;
        let body = &report[2..2 + size];
        // If serial out/err arrives while we're in a command, you may want to tee it somewhere.
        let cls = tag & HF2_FLAG_MASK;
        if cls == HF2_FLAG_CMDPKT_BODY || cls == HF2_FLAG_CMDPKT_LAST {
            buf_accum.extend_from_slice(body);
            if cls == HF2_FLAG_CMDPKT_LAST {
                return Ok(());
            }
        } else {
            // Serial packet; ignore for "info" flow.
            // You could print to stdout/stderr if desired.
        }
    }
}

// Build HF2 command frame (8-byte header + payload), send, then read response.
fn talk_hid<T: UsbContext>(
    handle: &DeviceHandle<T>,
    ep_in: u8,
    ep_out: u8,
    seq_no: &mut u16,
    cmd: u32,
    data: Option<&[u8]>,
    timeout: Duration,
    rx_buf: &mut Vec<u8>,
) -> rusb::Result<()> {
    let mut tx = Vec::with_capacity(8 + data.map_or(0, |d| d.len()));
    tx.resize(8, 0);
    w32(&mut tx, 0, cmd);
    *seq_no = seq_no.wrapping_add(1);
    w16(&mut tx, 4, *seq_no);
    w16(&mut tx, 6, 0); // status/reserved
    if let Some(d) = data {
        tx.extend_from_slice(d);
    }

    // HF2_CMD_RESET_INTO_APP expects no response; we still try to write it.
    send_hid(handle, ep_out, &tx, timeout)?;
    if cmd == HF2_CMD_RESET_INTO_APP {
        return Ok(());
    }

    recv_hid(handle, ep_in, timeout, rx_buf)?;

    // Validate seq/status in first 4 bytes of response payload
    if rx_buf.len() < 4 {
        return Err(rusb::Error::Other);
    }
    let rseq = r16(rx_buf, 0);
    let rstat = r16(rx_buf, 2);
    if rseq != *seq_no || rstat != 0 {
        return Err(rusb::Error::Other);
    }
    Ok(())
}

fn main() -> io::Result<()> {
    // ---------- locate and open the device ----------
    let ctx = Context::new().expect("libusb context");
    let mut found = None;

    for dev in ctx.devices().expect("list devices").iter() {
        let desc = dev.device_descriptor().unwrap();
        println!(
            "desc: {:#x}:{:#x} - {:?} | {:#x} {:#x} {:#x}",
            desc.vendor_id(),
            desc.product_id(),
            desc.device_version(),
            desc.class_code(),
            desc.sub_class_code(),
            desc.protocol_code()
        );
        // Same heuristic the C used: (release_number & 0xff00) == 0x4200

        // Walk configs and interfaces
        for i in 0..desc.num_configurations() {
            let config = dev.config_descriptor(i).unwrap();
            for interface in config.interfaces() {
                for iface_desc in interface.descriptors() {
                    println!(
                        "  Interface class=0x{:x} subclass=0x{:x} proto=0x{:x}",
                        iface_desc.class_code(),
                        iface_desc.sub_class_code(),
                        iface_desc.protocol_code()
                    );

                    // Mass Storage signature: 08 / 06 / 50
                    if iface_desc.class_code() == 0x08
                        && iface_desc.sub_class_code() == 0x06
                        && iface_desc.protocol_code() == 0x50
                    {
                        println!("  -> Looks like a UF2 Mass Storage device!");
                        found = Some(dev.clone());
                    }
                }
            }
        }
    }

    let dev = match found {
        Some(d) => d,
        None => {
            eprintln!("no devices");
            std::process::exit(1);
        }
    };

    let mut handle = dev.open().expect("open device");

    // ---------- find interface + endpoints ----------
    let (iface, ep_in, ep_out) = find_hf2_endpoints(&handle).expect("find endpoints");

    // Detach kernel driver if bound (Linux)
    if handle.kernel_driver_active(iface).unwrap_or(false) {
        handle.detach_kernel_driver(iface).ok();
    }
    handle.claim_interface(iface).expect("claim interface");

    // ---------- HF2 “info section” ----------
    let timeout = Duration::from_millis(1000);
    let mut seq_no: u16 = 0;
    let mut rx = Vec::with_capacity(4096);

    // HF2_CMD_INFO
    talk_hid(
        &handle,
        ep_in,
        ep_out,
        &mut seq_no,
        HF2_CMD_INFO,
        None,
        timeout,
        &mut rx,
    )
    .expect("HF2_CMD_INFO");
    // First 4 bytes are seq/status we already validated; the printable string starts at +4.
    let info_str = String::from_utf8_lossy(&rx[4..]).to_string();
    println!("INFO: {}", info_str);

    // HF2_CMD_START_FLASH (no params)
    talk_hid(
        &handle,
        ep_in,
        ep_out,
        &mut seq_no,
        HF2_CMD_START_FLASH,
        None,
        timeout,
        &mut rx,
    )
    .expect("HF2_CMD_START_FLASH");

    // HF2_CMD_BININFO
    talk_hid(
        &handle,
        ep_in,
        ep_out,
        &mut seq_no,
        HF2_CMD_BININFO,
        None,
        timeout,
        &mut rx,
    )
    .expect("HF2_CMD_BININFO");

    // Typical BININFO layout (after seq/status):
    // [0] mode, [1] res, [2..6] reserved, [8..12] pageSize, [12..16] numPages, [16..20] maxMsg
    if rx.len() < 20 {
        eprintln!("BININFO response too short");
        std::process::exit(1);
    }
    let mode = rx[4];
    if mode != HF2_MODE_BOOTLOADER {
        eprintln!("not bootloader");
        std::process::exit(1);
    }
    let page_size = r32(&rx, 8);
    let num_pages = r32(&rx, 12);
    let msg_size = r32(&rx, 16);
    let total_kb = (page_size as u64 * num_pages as u64) / 1024;

    println!("page size: {page_size}, total: {total_kb}kB, msg size: {msg_size}");

    // Optional: reset into app (will not respond)
    // talk_hid(&handle, ep_in, ep_out, &mut seq_no, HF2_CMD_RESET_INTO_APP, None, timeout, &mut rx).ok();

    // Cleanup
    handle.release_interface(iface).ok();
    Ok(())
}
