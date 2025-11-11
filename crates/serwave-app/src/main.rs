slint::include_modules!();

use anyhow::Result;
use serwave_core::{SerialConfig, SerialService, SerialEvent, LogStore, Direction, TextEncoding};
use std::rc::Rc;
use std::cell::RefCell;
use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::io::Write as IoWrite;

#[derive(Serialize, Deserialize, Clone)]
struct SendPreset {
    name: String,
    data: String,
    is_hex: bool,
}

fn get_presets_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("serwave");
    fs::create_dir_all(&path).ok();
    path.push("presets.json");
    path
}

fn load_presets() -> Vec<SendPreset> {
    let path = get_presets_path();
    if let Ok(content) = fs::read_to_string(path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn save_presets(presets: &[SendPreset]) {
    let path = get_presets_path();
    if let Ok(json) = serde_json::to_string_pretty(presets) {
        let _ = fs::write(path, json);
    }
}

fn get_log_path() -> PathBuf {
    let mut path = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("serwave");
    fs::create_dir_all(&path).ok();
    path.push("serial.log");
    path
}

struct LogWriter {
    file: Arc<Mutex<Option<fs::File>>>,
}

impl LogWriter {
    fn new() -> Self {
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(get_log_path())
            .ok();
        Self {
            file: Arc::new(Mutex::new(file)),
        }
    }

    fn write_entry(&self, direction: Direction, data: &[u8]) {
        let file_clone = self.file.clone();
        let data = data.to_vec();
        std::thread::spawn(move || {
            if let Ok(mut file_guard) = file_clone.lock() {
                if let Some(file) = file_guard.as_mut() {
                    let prefix = match direction {
                        Direction::Rx => b"RX: ",
                        Direction::Tx => b"TX: ",
                    };
                    let _ = file.write_all(prefix);
                    let _ = file.write_all(&data);
                    let _ = file.write_all(b"\n");
                    let _ = file.flush();
                }
            }
        });
    }
}

fn main() -> Result<()> {
    let app = MainWindow::new()?;

    let log_store = Rc::new(RefCell::new(LogStore::new(10000)));
    let serial_service: Rc<RefCell<Option<SerialService>>> = Rc::new(RefCell::new(None));
    let rx_buffer: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
    let last_rx_time: Rc<RefCell<Option<std::time::Instant>>> = Rc::new(RefCell::new(None));
    let presets: Rc<RefCell<Vec<SendPreset>>> = Rc::new(RefCell::new(load_presets()));
    let log_writer = Rc::new(LogWriter::new());

    // Initialize port list
    refresh_ports(&app);
    refresh_presets(&app, &presets.borrow());

    // Connect button
    {
        let app_weak = app.as_weak();
        let serial_service = serial_service.clone();
        let log_store = log_store.clone();

        app.on_connect_clicked(move || {
            let app = app_weak.unwrap();
            let port_display = app.get_selected_port().to_string();
            let baud_rate = app.get_baud_rate() as u32;

            if port_display.is_empty() {
                return;
            }

            let port_name = port_display.split_whitespace().next().unwrap_or(&port_display).to_string();

            let config = SerialConfig {
                port_name: port_name.clone(),
                baud_rate,
                ..Default::default()
            };

            match SerialService::open(config) {
                Ok(service) => {
                    *serial_service.borrow_mut() = Some(service);
                    app.set_is_connected(true);
                    log_store.borrow_mut().push(Direction::Tx, format!("已连接到 {}", port_name).into_bytes());
                    update_log_display(&app, &log_store.borrow());
                }
                Err(e) => {
                    log_store.borrow_mut().push(Direction::Tx, format!("连接失败: {}", e).into_bytes());
                    update_log_display(&app, &log_store.borrow());
                }
            }
        });
    }

    // Disconnect button
    {
        let app_weak = app.as_weak();
        let serial_service = serial_service.clone();
        let log_store = log_store.clone();
        let rx_buffer = rx_buffer.clone();
        let last_rx_time = last_rx_time.clone();

        app.on_disconnect_clicked(move || {
            let app = app_weak.unwrap();
            if let Some(service) = serial_service.borrow().as_ref() {
                service.close();
            }
            *serial_service.borrow_mut() = None;
            rx_buffer.borrow_mut().clear();
            *last_rx_time.borrow_mut() = None;
            app.set_is_connected(false);
            log_store.borrow_mut().push(Direction::Tx, "已断开连接".as_bytes().to_vec());
            update_log_display(&app, &log_store.borrow());
        });
    }

    // Send button
    {
        let app_weak = app.as_weak();
        let serial_service = serial_service.clone();
        let log_store = log_store.clone();
        let log_writer = log_writer.clone();

        app.on_send_clicked(move |text| {
            let app = app_weak.unwrap();
            if let Some(service) = serial_service.borrow().as_ref() {
                let data = if app.get_hex_send_mode() {
                    let hex_str = text.to_string().replace(" ", "");
                    match hex::decode(&hex_str) {
                        Ok(bytes) => bytes,
                        Err(_) => {
                            log_store.borrow_mut().push(Direction::Tx, "HEX格式错误".as_bytes().to_vec());
                            update_log_display(&app, &log_store.borrow());
                            return;
                        }
                    }
                } else {
                    let mut data = text.to_string().into_bytes();
                    data.extend_from_slice(service.config().line_ending.as_bytes());
                    data
                };

                log_store.borrow_mut().push(Direction::Tx, data.clone());
                log_writer.write_entry(Direction::Tx, &data);
                let _ = service.send(data);
                update_log_display(&app, &log_store.borrow());
            }
        });
    }

    // Clear button
    {
        let app_weak = app.as_weak();
        let log_store = log_store.clone();

        app.on_clear_clicked(move || {
            let app = app_weak.unwrap();
            log_store.borrow_mut().clear();
            update_log_display(&app, &log_store.borrow());
        });
    }

    // Refresh ports button
    {
        let app_weak = app.as_weak();
        app.on_refresh_ports_clicked(move || {
            let app = app_weak.unwrap();
            refresh_ports(&app);
        });
    }

    // DTR toggle
    {
        let serial_service = serial_service.clone();
        app.on_dtr_toggled(move |state| {
            if let Some(service) = serial_service.borrow().as_ref() {
                let _ = service.set_dtr(state);
            }
        });
    }

    // RTS toggle
    {
        let serial_service = serial_service.clone();
        app.on_rts_toggled(move |state| {
            if let Some(service) = serial_service.borrow().as_ref() {
                let _ = service.set_rts(state);
            }
        });
    }

    // Encoding changed
    {
        let app_weak = app.as_weak();
        let log_store = log_store.clone();
        app.on_encoding_changed(move |_encoding| {
            let app = app_weak.unwrap();
            update_log_display(&app, &log_store.borrow());
        });
    }

    // Display options changed
    {
        let app_weak = app.as_weak();
        let log_store = log_store.clone();
        app.on_display_options_changed(move || {
            let app = app_weak.unwrap();
            let keywords: Vec<String> = app.get_highlight_keywords()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            log_store.borrow_mut().set_highlight_keywords(keywords);
            log_store.borrow_mut().set_filter(app.get_show_rx(), app.get_show_tx());
            update_log_display(&app, &log_store.borrow());
        });
    }

    // Preset selected
    {
        let presets = presets.clone();
        let serial_service = serial_service.clone();
        let log_store = log_store.clone();
        let log_writer = log_writer.clone();
        app.on_preset_selected(move |name| {
            if let Some(preset) = presets.borrow().iter().find(|p| p.name == name.as_str()) {
                if let Some(service) = serial_service.borrow().as_ref() {
                    let data = if preset.is_hex {
                        let hex_str = preset.data.replace(" ", "");
                        match hex::decode(&hex_str) {
                            Ok(bytes) => bytes,
                            Err(_) => return,
                        }
                    } else {
                        let mut data = preset.data.as_bytes().to_vec();
                        data.extend_from_slice(service.config().line_ending.as_bytes());
                        data
                    };
                    log_store.borrow_mut().push(Direction::Tx, data.clone());
                    log_writer.write_entry(Direction::Tx, &data);
                    let _ = service.send(data);
                }
            }
        });
    }

    // Save preset
    {
        let app_weak = app.as_weak();
        let presets = presets.clone();
        app.on_save_preset_clicked(move |name, data, is_hex| {
            let app = app_weak.unwrap();
            let mut presets_mut = presets.borrow_mut();
            if let Some(existing) = presets_mut.iter_mut().find(|p| p.name == name.as_str()) {
                existing.data = data.to_string();
                existing.is_hex = is_hex;
            } else {
                presets_mut.push(SendPreset {
                    name: name.to_string(),
                    data: data.to_string(),
                    is_hex,
                });
            }
            save_presets(&presets_mut);
            refresh_presets(&app, &presets_mut);
        });
    }

    // Delete preset
    {
        let app_weak = app.as_weak();
        let presets = presets.clone();
        app.on_delete_preset_clicked(move |name| {
            let app = app_weak.unwrap();
            let mut presets_mut = presets.borrow_mut();
            presets_mut.retain(|p| p.name != name.as_str());
            save_presets(&presets_mut);
            refresh_presets(&app, &presets_mut);
            app.set_selected_preset("".into());
        });
    }

    // Event polling timer
    let app_weak = app.as_weak();
    let serial_service_clone = serial_service.clone();
    let log_store_clone = log_store.clone();
    let rx_buffer_clone = rx_buffer.clone();
    let last_rx_time_clone = last_rx_time.clone();
    let log_writer_clone = log_writer.clone();

    let _timer = slint::Timer::default();
    _timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(50), move || {
            let app = app_weak.unwrap();
            if let Some(service) = serial_service_clone.borrow().as_ref() {
                while let Ok(event) = service.events().try_recv() {
                    match event {
                        SerialEvent::Rx(data) => {
                            let now = std::time::Instant::now();
                            let mut buf = rx_buffer_clone.borrow_mut();
                            let mut last_time = last_rx_time_clone.borrow_mut();

                            let force_flush = if let Some(last) = *last_time {
                                now.duration_since(last).as_millis() > 100
                            } else {
                                false
                            };

                            buf.extend_from_slice(&data);

                            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                                let line: Vec<u8> = buf.drain(..=pos).collect();
                                log_store_clone.borrow_mut().push(Direction::Rx, line.clone());
                                log_writer_clone.write_entry(Direction::Rx, &line);
                            }

                            if (force_flush && !buf.is_empty()) || buf.len() > 1024 {
                                let line = buf.drain(..).collect::<Vec<u8>>();
                                log_store_clone.borrow_mut().push(Direction::Rx, line.clone());
                                log_writer_clone.write_entry(Direction::Rx, &line);
                            }

                            *last_time = Some(now);
                            update_log_display(&app, &log_store_clone.borrow());
                        }
                        SerialEvent::Error(e) => {
                            log_store_clone.borrow_mut().push(Direction::Tx, format!("错误: {}", e).into_bytes());
                            update_log_display(&app, &log_store_clone.borrow());
                        }
                        SerialEvent::Closed => {
                            app.set_is_connected(false);
                        }
                        SerialEvent::PinStates(states) => {
                            app.set_cts_status(states.cts);
                            app.set_dsr_status(states.dsr);
                            app.set_dcd_status(states.dcd);
                            app.set_ri_status(states.ri);
                        }
                        _ => {}
                    }
                }
                let _ = service.request_pin_states();
            }
        });

    app.run()?;
    Ok(())
}

fn refresh_ports(app: &MainWindow) {
    let ports = SerialService::list_ports();
    let port_names: Vec<slint::SharedString> = ports.iter().map(|p| {
        if let (Some(vid), Some(pid)) = (p.vid, p.pid) {
            format!("{} ({:04X}:{:04X})", p.port_name, vid, pid).into()
        } else {
            p.port_name.clone().into()
        }
    }).collect();

    let port_list = Rc::new(slint::VecModel::from(port_names.clone()));
    app.set_port_list(port_list.into());

    if !port_names.is_empty() && app.get_selected_port().is_empty() {
        app.set_selected_port(port_names[0].clone());
    }
}

fn refresh_presets(app: &MainWindow, presets: &[SendPreset]) {
    let preset_names: Vec<slint::SharedString> = presets.iter().map(|p| p.name.clone().into()).collect();
    let preset_list = Rc::new(slint::VecModel::from(preset_names));
    app.set_preset_list(preset_list.into());
}

fn update_log_display(app: &MainWindow, log_store: &LogStore) {
    let show_timestamp = app.get_show_timestamp();
    let show_hex = app.get_show_hex();
    let encoding = app.get_selected_encoding().as_str().parse().unwrap_or(TextEncoding::Auto);
    let text = log_store.to_text_with_encoding(show_timestamp, show_hex, encoding);
    app.set_log_text(text.into());
}
