use serde::Serialize;
use std::io::{self, BufRead};

#[derive(Serialize)]
struct Response { event: &'static str, status: &'static str, platform: &'static str }

fn main() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let response = match line.trim() {
            "ping" => Response { event: "pong", status: "ready", platform: "windows" },
            "usb:list" => Response { event: "usb:list", status: "not-implemented", platform: "windows" },
            _ => Response { event: "error", status: "unknown-command", platform: "windows" },
        };
        if let Ok(json) = serde_json::to_string(&response) { println!("{json}"); }
    }
}
