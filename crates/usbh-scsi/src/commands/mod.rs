pub mod cbw;
pub mod inquiry;
pub mod read10;
pub mod read_capacity;
pub mod write10;

pub trait CommandBlock {
    /// Return the command bytes (CDB).
    fn to_bytes(&self) -> [u8; 16];

    /// Return the effective length of the command.
    fn len(&self) -> u8;
}
