pub mod cbw;
pub mod inquiry;

pub trait CommandBlock {
    /// Return the command bytes (CDB).
    fn to_bytes(&self) -> [u8; 16];

    /// Return the effective length of the command.
    fn len(&self) -> u8;
}
