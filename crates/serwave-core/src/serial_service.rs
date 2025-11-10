use crossbeam_channel::{unbounded, Receiver, Sender};
use serialport::SerialPortInfo;
use std::io::{Read, Write};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub port_name: String,
    pub port_type: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

impl From<SerialPortInfo> for PortInfo {
    fn from(info: SerialPortInfo) -> Self {
        let (port_type, vid, pid, serial_number, manufacturer, product) = match &info.port_type {
            serialport::SerialPortType::UsbPort(usb) => (
                "USB".to_string(),
                Some(usb.vid),
                Some(usb.pid),
                usb.serial_number.clone(),
                usb.manufacturer.clone(),
                usb.product.clone(),
            ),
            serialport::SerialPortType::PciPort => ("PCI".to_string(), None, None, None, None, None),
            serialport::SerialPortType::BluetoothPort => ("Bluetooth".to_string(), None, None, None, None, None),
            serialport::SerialPortType::Unknown => ("Unknown".to_string(), None, None, None, None, None),
        };
        Self {
            port_name: info.port_name,
            port_type,
            vid,
            pid,
            serial_number,
            manufacturer,
            product,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineEnding {
    LF,
    CR,
    CRLF,
}

impl LineEnding {
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            LineEnding::LF => b"\n",
            LineEnding::CR => b"\r",
            LineEnding::CRLF => b"\r\n",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SerialConfig {
    pub port_name: String,
    pub baud_rate: u32,
    pub data_bits: serialport::DataBits,
    pub parity: serialport::Parity,
    pub stop_bits: serialport::StopBits,
    pub flow_control: serialport::FlowControl,
    pub line_ending: LineEnding,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port_name: String::new(),
            baud_rate: 115_200,
            data_bits: serialport::DataBits::Eight,
            parity: serialport::Parity::None,
            stop_bits: serialport::StopBits::One,
            flow_control: serialport::FlowControl::None,
            line_ending: LineEnding::LF,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SerialEvent {
    Rx(Vec<u8>),
    Tx(usize),
    Opened(String),
    Closed,
    Error(String),
    PinStates(PinStates),
}

enum Command {
    Send(Vec<u8>),
    Close,
    SetDtr(bool),
    SetRts(bool),
    GetPinStates,
}

#[derive(Debug, Clone)]
pub struct PinStates {
    pub cts: bool,
    pub dsr: bool,
    pub dcd: bool,
    pub ri: bool,
}

pub struct SerialService {
    cfg: SerialConfig,
    tx_cmd: Sender<Command>,
    rx_evt: Receiver<SerialEvent>,
}

impl SerialService {
    pub fn list_ports() -> Vec<PortInfo> {
        serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .map(PortInfo::from)
            .collect()
    }

    pub fn open(cfg: SerialConfig) -> Result<Self, String> {
        let (tx_cmd, rx_cmd) = unbounded::<Command>();
        let (tx_evt, rx_evt) = unbounded::<SerialEvent>();
        let cfg_clone = cfg.clone();

        std::thread::spawn(move || {
            match serialport::new(&cfg_clone.port_name, cfg_clone.baud_rate)
                .data_bits(cfg_clone.data_bits)
                .parity(cfg_clone.parity)
                .stop_bits(cfg_clone.stop_bits)
                .flow_control(cfg_clone.flow_control)
                .timeout(Duration::from_millis(50))
                .open()
            {
                Ok(mut port) => {
                    let _ = tx_evt.send(SerialEvent::Opened(cfg_clone.port_name.clone()));
                    let mut buf = [0u8; 4096];
                    loop {
                        match port.read(&mut buf) {
                            Ok(n) if n > 0 => {
                                let _ = tx_evt.send(SerialEvent::Rx(buf[..n].to_vec()));
                            }
                            Ok(_) | Err(_) => {}
                        }
                        while let Ok(cmd) = rx_cmd.try_recv() {
                            match cmd {
                                Command::Send(data) => {
                                    match port.write(&data) {
                                        Ok(n) => { let _ = tx_evt.send(SerialEvent::Tx(n)); }
                                        Err(e) => { let _ = tx_evt.send(SerialEvent::Error(e.to_string())); }
                                    }
                                }
                                Command::SetDtr(state) => {
                                    if let Err(e) = port.write_data_terminal_ready(state) {
                                        let _ = tx_evt.send(SerialEvent::Error(e.to_string()));
                                    }
                                }
                                Command::SetRts(state) => {
                                    if let Err(e) = port.write_request_to_send(state) {
                                        let _ = tx_evt.send(SerialEvent::Error(e.to_string()));
                                    }
                                }
                                Command::GetPinStates => {
                                    let states = PinStates {
                                        cts: port.read_clear_to_send().unwrap_or(false),
                                        dsr: port.read_data_set_ready().unwrap_or(false),
                                        dcd: port.read_carrier_detect().unwrap_or(false),
                                        ri: port.read_ring_indicator().unwrap_or(false),
                                    };
                                    let _ = tx_evt.send(SerialEvent::PinStates(states));
                                }
                                Command::Close => {
                                    let _ = tx_evt.send(SerialEvent::Closed);
                                    return;
                                }
                            }
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                Err(e) => {
                    let _ = tx_evt.send(SerialEvent::Error(format!("open failed: {e}")));
                    let _ = tx_evt.send(SerialEvent::Closed);
                }
            }
        });

        Ok(Self { cfg, tx_cmd, rx_evt })
    }

    pub fn send(&self, data: Vec<u8>) -> Result<(), String> {
        self.tx_cmd.send(Command::Send(data)).map_err(|e| e.to_string())
    }

    pub fn set_dtr(&self, state: bool) -> Result<(), String> {
        self.tx_cmd.send(Command::SetDtr(state)).map_err(|e| e.to_string())
    }

    pub fn set_rts(&self, state: bool) -> Result<(), String> {
        self.tx_cmd.send(Command::SetRts(state)).map_err(|e| e.to_string())
    }

    pub fn request_pin_states(&self) -> Result<(), String> {
        self.tx_cmd.send(Command::GetPinStates).map_err(|e| e.to_string())
    }

    pub fn close(&self) {
        let _ = self.tx_cmd.send(Command::Close);
    }

    pub fn events(&self) -> &Receiver<SerialEvent> {
        &self.rx_evt
    }

    pub fn config(&self) -> &SerialConfig { &self.cfg }
}

