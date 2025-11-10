use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: u64,
    pub direction: Direction,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Rx,
    Tx,
}

pub struct LogStore {
    entries: Vec<LogEntry>,
    max_entries: usize,
}

impl LogStore {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }

    pub fn push(&mut self, direction: Direction, data: Vec<u8>) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        self.entries.push(LogEntry {
            timestamp,
            direction,
            data,
        });

        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn to_text(&self, show_timestamp: bool, show_hex: bool) -> String {
        self.to_text_with_encoding(show_timestamp, show_hex, crate::TextEncoding::Auto)
    }

    pub fn to_text_with_encoding(&self, show_timestamp: bool, show_hex: bool, encoding: crate::TextEncoding) -> String {
        let mut result = String::new();
        for entry in &self.entries {
            let prefix = match entry.direction {
                Direction::Rx => "RX: ",
                Direction::Tx => "TX: ",
            };

            if show_hex {
                if show_timestamp {
                    let millis = entry.timestamp % 1000;
                    let now = SystemTime::now();
                    let elapsed = now.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
                    let offset_ms = elapsed - entry.timestamp;
                    let local_time = now - std::time::Duration::from_millis(offset_ms);

                    if let Ok(duration) = local_time.duration_since(UNIX_EPOCH) {
                        let total_secs = duration.as_secs();
                        let local_secs = total_secs + (8 * 3600);
                        let hours = (local_secs / 3600) % 24;
                        let minutes = (local_secs / 60) % 60;
                        let seconds = local_secs % 60;
                        result.push_str(&format!("[{hours:02}:{minutes:02}:{seconds:02}.{millis:03}] "));
                    }
                }
                result.push_str(prefix);
                for byte in &entry.data {
                    result.push_str(&format!("{byte:02X} "));
                }
                result.push('\n');
            } else {
                let text = encoding.decode(&entry.data);
                if text.trim().is_empty() {
                    continue;
                }

                if show_timestamp {
                    let millis = entry.timestamp % 1000;
                    let now = SystemTime::now();
                    let elapsed = now.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
                    let offset_ms = elapsed - entry.timestamp;
                    let local_time = now - std::time::Duration::from_millis(offset_ms);

                    if let Ok(duration) = local_time.duration_since(UNIX_EPOCH) {
                        let total_secs = duration.as_secs();
                        let local_secs = total_secs + (8 * 3600);
                        let hours = (local_secs / 3600) % 24;
                        let minutes = (local_secs / 60) % 60;
                        let seconds = local_secs % 60;
                        result.push_str(&format!("[{hours:02}:{minutes:02}:{seconds:02}.{millis:03}] "));
                    }
                }
                result.push_str(prefix);
                result.push_str(&text);
                if !text.ends_with('\n') {
                    result.push('\n');
                }
            }
        }
        result
    }
}
