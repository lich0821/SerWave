pub mod i2c;
pub mod spi;
pub mod uart;

#[derive(Debug, Clone, Copy)]
pub struct SampleRate(pub f64); // Hz

#[derive(Debug, Clone, Copy)]
pub struct TimeSpan {
    pub start_s: f64,
    pub end_s: f64,
}
