// sudo adduser <user> dialout
// sudo chmod a+rw /dev/ttyACM0
//
// https://medium.com/@rajeshpachaikani/connect-esp32-to-wifi-with-rust-7d12532f539b
// https://github.com/esp-rs/std-training/blob/main/intro/http-server/examples/http_server.rs
// https://esp-rs.github.io/book/

use std::{
    sync::{Arc, Mutex},
    thread,
    thread::sleep,
    time::Duration,
};
use serde_json;
use embedded_svc::{http::Method, io::Write};
use esp_idf_sys as _;
use esp_idf_hal::{
    peripherals::Peripherals,
};
use esp_idf_svc::{
    wifi::EspWifi,
    nvs::EspDefaultNvsPartition,
    eventloop::EspSystemEventLoop,
    http::server::{EspHttpServer}
};
use embedded_svc::wifi::{ClientConfiguration, Wifi, Configuration};
use esp_idf_svc::http::server::Configuration as SVC_Configuration;
use device::{ Device, Action };

fn main(){
    esp_idf_sys::link_patches();//Needed for esp32-rs
    println!("Entered Main function!");
    let peripherals = Peripherals::take().unwrap();
    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    let mut wifi_driver = EspWifi::new(
        peripherals.modem,
        sys_loop,
        Some(nvs)
    ).unwrap();

    wifi_driver.set_configuration(&Configuration::Client(ClientConfiguration{
        ssid: "ssid".into(),
        password: "password".into(),
        ..Default::default()
    })).unwrap();

    wifi_driver.start().unwrap();
    wifi_driver.connect().unwrap();
    while !wifi_driver.is_connected().unwrap(){
        let config = wifi_driver.get_configuration().unwrap();
        println!("Waiting for station {:?}", config);
    }
    println!("Should be connected now");

    let roof_vent = Arc::new(Mutex::new(Device {
        name: "roof vent".to_string(),
        action: Action::Off,
        available_actions: Vec::from([Action::On, Action::Off]),
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
    server.fn_handler("/", Method::Get, |request| {
                println!("thing recieved!!!!!!!!!!!!!!!!!!!");
                println!("request uri: {}", request.uri());
                let html = index_html();
                let mut response = request.into_ok_response()?;
                response.write_all(html.as_bytes())?;
                Ok(())
            }).unwrap();
    let roof_vent_clone = roof_vent.clone();
    let vent_louver_clone = vent_louver.clone();
    server.fn_handler("/devices", Method::Get, move |request| {
                let roof_vent_guard = roof_vent_clone.lock().unwrap().clone();
                let vent_louver_guard = vent_louver_clone.lock().unwrap().clone();
                let payload = serde_json::json!({roof_vent_guard.name.clone():
                                              roof_vent_guard,
                                              vent_louver_guard.name.clone():
                                              vent_louver_guard});
                let mut response = request.into_ok_response()?;
                response.write_all(payload.to_string().as_bytes())?;
                Ok(())
            }).unwrap();
    loop{
        println!("IP info: {:?}", wifi_driver.sta_netif().get_ip_info().unwrap());
        sleep(Duration::new(10,0));
    }
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
