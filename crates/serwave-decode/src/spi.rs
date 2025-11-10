/// Minimal SPI decoder trait & stub structures (to be implemented).
pub struct SpiFrame {
    pub cpol: bool,
    pub cpha: bool,
    pub bytes: Vec<u8>,
}
