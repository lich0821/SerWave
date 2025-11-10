/// Minimal I2C decoder trait & stub structures (to be implemented).
pub struct I2cFrame {
    pub address: u8,
    pub rw: bool,
    pub data: Vec<u8>,
    pub acked: bool,
}
