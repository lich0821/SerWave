/// Minimal UART parser traits and error placeholders (to be implemented).
pub struct UartChar {
    pub byte: u8,
    pub parity_ok: bool,
}
