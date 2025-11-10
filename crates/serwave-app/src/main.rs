slint::include_modules!();

use anyhow::Result;
use serwave_core::{SerialConfig, SerialService, SerialEvent, LogStore, Direction, TextEncoding};
use std::rc::Rc;
use std::cell::RefCell;

fn main() -> Result<()> {
    let app = MainWindow::new()?;

    let log_store = Rc::new(RefCell::new(LogStore::new(10000)));
    let serial_service: Rc<RefCell<Option<SerialService>>> = Rc::new(RefCell::new(None));
    let rx_buffer: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
    let last_rx_time: Rc<RefCell<Option<std::time::Instant>>> = Rc::new(RefCell::new(None));

    // Initialize port list
    refresh_ports(&app);

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

        app.on_send_clicked(move |text| {
            let app = app_weak.unwrap();
            if let Some(service) = serial_service.borrow().as_ref() {
                let mut data = text.to_string().into_bytes();
                data.extend_from_slice(service.config().line_ending.as_bytes());

                log_store.borrow_mut().push(Direction::Tx, data.clone());
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
            update_log_display(&app, &log_store.borrow());
        });
    }

    // Event polling timer
    let app_weak = app.as_weak();
    let serial_service_clone = serial_service.clone();
    let log_store_clone = log_store.clone();
    let rx_buffer_clone = rx_buffer.clone();
    let last_rx_time_clone = last_rx_time.clone();

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
                                log_store_clone.borrow_mut().push(Direction::Rx, line);
                            }

                            if (force_flush && !buf.is_empty()) || buf.len() > 1024 {
                                let line = buf.drain(..).collect();
                                log_store_clone.borrow_mut().push(Direction::Rx, line);
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

fn update_log_display(app: &MainWindow, log_store: &LogStore) {
    let show_timestamp = app.get_show_timestamp();
    let show_hex = app.get_show_hex();
    let encoding = app.get_selected_encoding().as_str().parse().unwrap_or(TextEncoding::Auto);
    let text = log_store.to_text_with_encoding(show_timestamp, show_hex, encoding);
    app.set_log_text(text.into());
}
