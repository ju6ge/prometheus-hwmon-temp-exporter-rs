use std::fs;

use actix_web::{web, App, HttpResponse, HttpServer};
use once_cell::sync::Lazy;
use regex::Regex;

static TEMPX_FILE_MATCHER: Lazy<Regex> = Lazy::new(|| Regex::new("temp(\\d+)_input").unwrap());

#[derive(Debug)]
struct TemperatureMetricData {
    name: String,
    device: String,
    label: String,
    value: f64
}


async fn prometheus_response() -> HttpResponse {
    let temps_resp = actix_web::rt::task::spawn_blocking(|| {
        let mut data: Vec<TemperatureMetricData> = Vec::new();
        let mon_dirs = fs::read_dir("/sys/class/hwmon/")
            .unwrap()
            .filter(|dir| {
                dir.as_ref().is_ok_and(|path| {
                    let entries = fs::read_dir(path.path()).unwrap();

                    for e in entries {
                        if e.is_ok_and(|entry| {
                            entry.file_type().is_ok_and(|t| t.is_file())
                                && entry
                                    .file_name()
                                    .to_str()
                                    .is_some_and(|name| name.starts_with("temp"))
                        }) {
                            return true;
                        }
                    }
                    false
                })
            })
            .filter(|dir| {
                dir.as_ref()
                    .is_ok_and(|path| path.path().join("name").exists())
            })
            .filter_map(|d| d.ok());
        for d in mon_dirs {
            let name = fs::read_to_string(d.path().join("name"))
                .unwrap()
                .trim()
                .to_string();
            let device = d.path().join("device").canonicalize().unwrap().file_name().unwrap().to_os_string().into_string().unwrap();

            let entries = fs::read_dir(d.path()).unwrap().filter_map(|e| e.ok());

            for e in entries {
                match TEMPX_FILE_MATCHER.captures(e.file_name().to_str().unwrap()) {
                    Some(capture) => {
                        let x = capture.get(1).unwrap();

                        let input_path = d.path().join(format!("temp{}_input", x.as_str()));
                        let label_path = d.path().join(format!("temp{}_label", x.as_str()));

                        if input_path.exists() && label_path.exists() {
                            match fs::read_to_string(input_path) {
                                Ok(temp_input) =>  {
                                    let label = fs::read_to_string(label_path).unwrap().trim().to_string();
                                    let value: f64 = temp_input.trim().parse::<f64>().unwrap() / 1000.;

                                    data.push(TemperatureMetricData { name: name.clone(), device: device.clone(), label, value});
                                },
                                Err(_) => { /*ignore read errors here and go to next*/ },
                            }
                        }
                    },
                    None => { /*file not of interest*/ }
                }
            }
        }
        //println!("{data:#?}");
        data
    })
    .await;

    match temps_resp {
        Ok(data) => {
            let mut body = String::new();
            for temp_data in data {
                body = format!("{body}temp_{}{{device=\"{}\",label=\"{}\"}} {}\n", temp_data.name, temp_data.device, temp_data.label, temp_data.value);
            }
            HttpResponse::Ok().body(body)
        },
        Err(err) => HttpResponse::InternalServerError().body(err.to_string()),
    }
}

#[actix_web::main]
async fn main() -> Result<(), std::io::Error> {
    HttpServer::new(move || App::new().route("/metrics", web::to(prometheus_response)))
        .bind(("0.0.0.0", 19714))?
        .run()
        .await
}
