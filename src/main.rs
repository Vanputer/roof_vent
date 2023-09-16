// sudo adduser <user> dialout
// sudo chmod a+rw /dev/ttyACM0
//
// https://medium.com/@rajeshpachaikani/connect-esp32-to-wifi-with-rust-7d12532f539b
// https://github.com/esp-rs/std-training/blob/main/intro/http-server/examples/http_server.rs
// https://esp-rs.github.io/book/

use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};
use embedded_svc::{
    http::server::{HandlerError, Request},
    http::Method,
    io::Write,
};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::http::server::Configuration as SVC_Configuration;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::server::{EspHttpConnection, EspHttpServer},
    nvs::EspDefaultNvsPartition,
    wifi::EspWifi,
};
use esp_idf_sys as _;
use querystring;
use serde_json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
    thread::sleep,
    time::Duration,
};

use device::{Action, Device};

fn main() {
    esp_idf_sys::link_patches(); //Needed for esp32-rs
    println!("Entered Main function!");
    let peripherals = Peripherals::take().unwrap();
    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    let mut wifi_driver = EspWifi::new(peripherals.modem, sys_loop, Some(nvs)).unwrap();

    wifi_driver
        .set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: "ssid".into(),
            password: "password".into(),
            ..Default::default()
        }))
        .unwrap();

    wifi_driver.start().unwrap();
    wifi_driver.connect().unwrap();
    while !wifi_driver.is_connected().unwrap() {
        let config = wifi_driver.get_configuration().unwrap();
        println!("Waiting for station {:?}", config);
    }
    println!("Should be connected now");

    let roof_vent = Arc::new(Mutex::new(Device {
        name: "roof vent".to_string(),
        action: Action::Off,
        available_actions: Vec::from([
            Action::On,
            Action::Off,
            Action::Up,
            Action::Down,
            Action::Set,
        ]),
        default_target: 3,
        dutycycles: [0, 20, 40, 60, 80, 100],
        target: 0,
        period_ms: 100,
        on_duration_ms: 0,
    }));

    // roof_vent Manager
    let roof_vent_clone = roof_vent.clone();
    thread::spawn(move || {
        loop {
            {
                let mut roof_vent = roof_vent_clone.lock().unwrap();
                // Modify the roof_vent state or perform operations
                println!("roof_vent Manager: Current device is {:?}", *roof_vent);
            }
            sleep(Duration::from_millis(5000));
        }
    });

    let vent_louver = Arc::new(Mutex::new(Device {
        name: "vent louver".to_string(),
        action: Action::Off,
        available_actions: Vec::from([Action::On, Action::Off]),
        default_target: 3,
        dutycycles: [0, 20, 40, 60, 80, 100],
        target: 0,
        period_ms: 100,
        on_duration_ms: 0,
    }));

    // roof_vent Manager
    let vent_louver_clone = vent_louver.clone();
    thread::spawn(move || {
        loop {
            {
                let mut vent_louver = vent_louver_clone.lock().unwrap();
                // Modify the roof_vent state or perform operations
                println!("Vent Louver Manager: Current device is {:?}", *vent_louver);
            }
            sleep(Duration::from_millis(5000));
        }
    });

    let mut server = EspHttpServer::new(&SVC_Configuration::default()).unwrap();
    server
        .fn_handler("/", Method::Get, |request| {
            println!("thing recieved!!!!!!!!!!!!!!!!!!!");
            println!("request uri: {}", request.uri());
            let html = index_html();
            let mut response = request.into_ok_response()?;
            response.write_all(html.as_bytes())?;
            Ok(())
        })
        .unwrap();
    let roof_vent_clone = roof_vent.clone();
    let vent_louver_clone = vent_louver.clone();
    server
        .fn_handler("/devices", Method::Get, move |request| {
            let roof_vent_guard = roof_vent_clone.lock().unwrap().clone();
            let vent_louver_guard = vent_louver_clone.lock().unwrap().clone();
            let payload = serde_json::json!({
            roof_vent_guard.name.clone(): roof_vent_guard,
            vent_louver_guard.name.clone(): vent_louver_guard});
            let mut response = request.into_ok_response()?;
            response.write_all(payload.to_string().as_bytes())?;
            Ok(())
        })
        .unwrap();
    let roof_vent_clone = roof_vent.clone();
    let vent_louver_clone = vent_louver.clone();
    server
        .fn_handler("/set", Method::Get, move |request| {
            let roof_vent_guard = roof_vent_clone.lock().unwrap();
            let vent_louver_guard = vent_louver_clone.lock().unwrap();
            let query = &request.uri()[5..].to_string();
            let query: HashMap<_, _> = querystring::querify(query).into_iter().collect();
            let mut device = match query.get("device") {
                Some(d) => match *d {
                    "roof%20vent" => roof_vent_guard,
                    "vent%20louver" => vent_louver_guard,
                    _ => {
                        exit_early(request, "Bad Device name given", 422);
                        return Ok(());
                    }
                },
                None => {
                    exit_early(request, "No Device name given", 422);
                    return Ok(());
                }
            };
            let action = match query.get("action") {
                Some(a) => {
                    let action = Action::from_str(a);
                    match action {
                        Ok(a) => {
                            if !device.available_actions.contains(&a) {
                                exit_early(request, "Device doesn't support Action", 422);
                                return Ok(());
                            }
                            a
                        }
                        Err(_) => {
                            exit_early(request, "Bad Action given", 422);
                            return Ok(());
                        }
                    }
                }
                None => {
                    exit_early(request, "No Action given", 422);
                    return Ok(());
                }
            };
            let target = if action == Action::Set {
                match query.get("target") {
                    Some(t) => match t.parse::<usize>() {
                        Ok(n) => {
                            if n > 5 {
                                exit_early(request, "Target should be 0-5", 422);
                                return Ok(());
                            }
                            Some(n)
                        }
                        Err(_) => {
                            exit_early(request, "Target should be 0-5", 422);
                            return Ok(());
                        }
                    },
                    None => {
                        exit_early(request, "A target needed to be given", 422);
                        return Ok(());
                    }
                }
            } else {
                None
            };
            device.take_action(action, target);
            let mut response = request.into_ok_response()?;
            response.write_all(&device.to_json().into_bytes()[..]);
            //response.write_all(&format!("{:?}", &query).into_bytes()[..]);
            Ok(())
        })
        .unwrap();
    loop {
        println!(
            "IP info: {:?}",
            wifi_driver.sta_netif().get_ip_info().unwrap()
        );
        sleep(Duration::new(10, 0));
    }
}

fn exit_early<'a>(
    request: Request<&mut EspHttpConnection<'a>>,
    message: &str,
    code: u16,
) -> Result<(), HandlerError> {
    let mut response = request.into_status_response(422)?;
    response.write_all(message.as_bytes());
    Ok(())
}

fn index_html() -> String {
    templated("Hello from ESP32-C3!")
}

fn templated(content: impl AsRef<str>) -> String {
    format!(
        r#"
<!DOCTYPE html>
<html>
    <head>
        <meta charset="utf-8">
        <title>esp-rs web server</title>
    </head>
    <body>
        {}
    </body>
</html>
"#,
        content.as_ref()
    )
}
