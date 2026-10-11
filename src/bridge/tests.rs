//! Synthetic protocol fixtures; no fixture is factory calibration for a real unit.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

fn device(raw: &str, model: &str) -> Value {
    json!({"value": {
        "hardwareVersion": {"value": model},
        "calibration": {"value": raw},
        "hwid": {"value": "TEST-DISPLAY"},
        "defaultQuilt": {"value": "{\"quiltX\":8192,\"quiltY\":8192,\"tileX\":8,\"tileY\":9,\"quiltAspect\":1.777}"}
    }})
}

#[test]
fn preserves_unknown_models_raw_optical_cells_and_bridge_layout() {
    let raw = "{\"serial\":\"TEST-ONLY\",\"pitch\":{\"value\":49.125},\"subpixelCells\":[{\"ROffsetX\":0.25}],\"futureField\":42}";
    let response = json!({"payload": {"value": {
        "0": {"value": {"hardwareVersion": {"value": "thirdparty"}}},
        "1": device(raw, "MODEL-NOT-IN-CATALOG")
    }}});
    let displays = parse_displays(&response).unwrap();
    assert_eq!(displays.len(), 1);
    assert_eq!(displays[0].hardware_version, "MODEL-NOT-IN-CATALOG");
    assert_eq!(displays[0].serial, "TEST-ONLY");
    assert_eq!(displays[0].calibration, raw);
    let quilt = displays[0].quilt.as_ref().unwrap();
    assert_eq!(
        (quilt.columns, quilt.rows, quilt.width, quilt.height),
        (8, 9, 8192, 8192)
    );
}

#[test]
fn lists_multiple_units_by_serial_and_does_not_invent_absent_data() {
    let response = json!({"payload": {"value": {
        "0": device(r#"{"serial":"TEST-B"}"#, "portrait"),
        "1": device(r#"{"serial":"TEST-A"}"#, "go")
    }}});
    let displays = parse_displays(&response).unwrap();
    assert_eq!(
        displays
            .iter()
            .map(|d| d.serial.as_str())
            .collect::<Vec<_>>(),
        ["TEST-A", "TEST-B"]
    );
    assert!(
        parse_displays(&json!({"payload": {"value": {}}}))
            .unwrap()
            .is_empty()
    );
    assert!(
        parse_displays(&json!({"payload": {"value": {"0": device("bad JSON", "portrait")}}}))
            .is_err()
    );
}

#[test]
fn unreachable_bridge_returns_unavailable() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    drop(listener);
    assert!(matches!(discover_at(&base), Err(Error::Unavailable)));
}

#[test]
fn uses_official_session_envelopes_and_exits_even_after_invalid_calibration() {
    for raw in [r#"{"serial":"TEST-HTTP"}"#, "bad JSON"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for (endpoint, payload) in [
                ("enter_orchestration", json!("TEST-TOKEN")),
                (
                    "available_output_devices",
                    json!({"0": device(raw, "portrait")}),
                ),
                ("exit_orchestration", json!("TEST-TOKEN")),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let (header_len, body_len) = loop {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                    if request.ends_with(b"\r\n\r\n") {
                        let headers = String::from_utf8(request.clone()).unwrap();
                        let body_len: usize = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse().unwrap())
                            })
                            .unwrap();
                        break (request.len(), body_len);
                    }
                };
                request.resize(header_len + body_len, 0);
                stream.read_exact(&mut request[header_len..]).unwrap();
                let headers = String::from_utf8_lossy(&request[..header_len]);
                assert!(headers.starts_with(&format!("PUT /{endpoint} HTTP/1.1\r\n")));
                let body: Value = serde_json::from_slice(&request[header_len..]).unwrap();
                if endpoint != "enter_orchestration" {
                    assert_eq!(body.get("orchestration").unwrap(), "TEST-TOKEN");
                }
                let response =
                    json!({"status": {"value": "Completion"}, "payload": {"value": payload}})
                        .to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                )
                .unwrap();
            }
        });
        let result = discover_at(&base);
        if raw == "bad JSON" {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap()[0].calibration, raw);
        }
        server.join().unwrap();
    }
}
