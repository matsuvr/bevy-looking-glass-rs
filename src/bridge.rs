//! Device discovery through the official Bridge.js HTTP protocol on any OS.
//! See <https://github.com/Looking-Glass/Bridge.js/tree/main/src/library>.

use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const BRIDGE_URL: &str = "http://localhost:33334/";
static SESSION: AtomicU64 = AtomicU64::new(0);

/// Bridge could not be reached or returned invalid device data.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No running Bridge is reachable on the local API port.
    #[error("Looking Glass Bridge is unavailable")]
    Unavailable,
    /// The Bridge response cannot describe a usable device.
    #[error("could not acquire Looking Glass Bridge device data: {0}")]
    Response(String),
}

/// Quilt geometry reported by Bridge, independent of a bundled model catalog.
#[derive(Clone, Debug, Deserialize)]
pub struct Quilt {
    /// Full quilt width in pixels (may include unused edge pixels).
    #[serde(rename = "quiltX")]
    pub width: u32,
    /// Full quilt height in pixels.
    #[serde(rename = "quiltY")]
    pub height: u32,
    /// Number of view columns.
    #[serde(rename = "tileX")]
    pub columns: u32,
    /// Number of view rows.
    #[serde(rename = "tileY")]
    pub rows: u32,
}

/// An attached light-field display and its unmodified factory calibration.
#[derive(Clone, Debug)]
pub struct Display {
    /// Bridge device index (not an OS monitor index).
    pub index: u32,
    /// Physical desktop coordinates reported by Bridge, including negative origins.
    pub window_position: bevy::math::IVec2,
    /// Serial of the physical unit, used for selection across reconnects.
    pub serial: String,
    /// Display name reported by Bridge.
    pub name: String,
    /// Bridge's model identifier; unknown model names are accepted.
    pub hardware_version: String,
    /// Original raw visual.json string, including optical cell tables.
    pub calibration: String,
    /// Bridge's recommended layout; required by automatic live output.
    pub quilt: Option<Quilt>,
}

/// Query the installed, running Bridge. The caller should use a background worker.
/// A session is released after acquisition, including response-parse failures.
///
/// Returns [`Error::Unavailable`] if Bridge cannot be reached on its local HTTP
/// port. An empty list means Bridge responded but no calibrated light-field
/// display is attached. Regular monitors and units without calibration are omitted.
/// Device models are not restricted to the bundled catalog. The returned raw
/// calibration can be passed to [`crate::parse_calibration`].
/// Each HTTP request has a five-second timeout; this function does not start or
/// install Bridge and does not create a window or calibration file.
pub fn discover() -> Result<Vec<Display>, Error> {
    discover_at(BRIDGE_URL)
}

fn discover_at(base: &str) -> Result<Vec<Display>, Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .into();
    let session = request(
        &agent,
        base,
        "enter_orchestration",
        json!({"name": format!("bevy-looking-glass-devices-{}-{}", std::process::id(), SESSION.fetch_add(1, Ordering::Relaxed))}),
    )?;
    let token = session
        .pointer("/payload/value")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| Error::Response("missing orchestration token".into()))?;
    let result = request(
        &agent,
        base,
        "available_output_devices",
        json!({"orchestration": token}),
    )
    .and_then(|response| parse_displays(&response));
    let exit = request(
        &agent,
        base,
        "exit_orchestration",
        json!({"orchestration": token}),
    );
    match (result, exit) {
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(displays), Ok(_)) => Ok(displays),
    }
}

fn request(agent: &ureq::Agent, base: &str, endpoint: &str, body: Value) -> Result<Value, Error> {
    let mut response = agent
        .put(format!("{base}{endpoint}"))
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .map_err(|error| match error {
            ureq::Error::StatusCode(code) => Error::Response(format!("{endpoint}: HTTP {code}")),
            _ => Error::Unavailable,
        })?;
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|error| Error::Response(format!("{endpoint}: {error}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| Error::Response(format!("{endpoint}: {error}")))?;
    if value.pointer("/status/value").and_then(Value::as_str) != Some("Completion") {
        return Err(Error::Response(format!("{endpoint} did not complete")));
    }
    Ok(value)
}

fn parse_displays(response: &Value) -> Result<Vec<Display>, Error> {
    let devices = response
        .pointer("/payload/value")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::Response("missing device list".into()))?;
    let mut displays = Vec::new();
    for device in devices.values() {
        let hardware_version = device
            .pointer("/value/hardwareVersion/value")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Response("missing hardware version".into()))?;
        if hardware_version == "thirdparty" {
            continue;
        }
        let Some(raw) = device
            .pointer("/value/calibration/value")
            .and_then(Value::as_str)
            .filter(|raw| !raw.is_empty())
        else {
            continue;
        };
        let calibration: Value = serde_json::from_str(raw)
            .map_err(|error| Error::Response(format!("factory calibration: {error}")))?;
        let serial = calibration
            .get("serial")
            .and_then(Value::as_str)
            .filter(|serial| !serial.is_empty())
            .ok_or_else(|| Error::Response("calibration has no serial".into()))?;
        let quilt = device
            .pointer("/value/defaultQuilt/value")
            .and_then(Value::as_str)
            .filter(|raw| !raw.is_empty() && *raw != "{}")
            .map(serde_json::from_str)
            .transpose()
            .map_err(|error| Error::Response(format!("{serial} quilt: {error}")))?;
        let name = device
            .pointer("/value/hwid/value")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .unwrap_or(serial);
        displays.push(Display {
            index: device
                .pointer("/value/index/value")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| {
                    Error::Response(format!("{serial}: missing or invalid display index"))
                })?,
            window_position: {
                #[derive(Deserialize)]
                struct Coordinates {
                    x: i32,
                    y: i32,
                }
                let coords: Coordinates = serde_json::from_value(
                    device
                        .pointer("/value/windowCoords/value")
                        .cloned()
                        .ok_or_else(|| {
                            Error::Response(format!("{serial}: missing window coordinates"))
                        })?,
                )
                .map_err(|error| {
                    Error::Response(format!("{serial} window coordinates: {error}"))
                })?;
                bevy::math::IVec2::new(coords.x, coords.y)
            },
            serial: serial.to_owned(),
            name: name.to_owned(),
            hardware_version: hardware_version.to_owned(),
            calibration: raw.to_owned(),
            quilt,
        });
    }
    displays.sort_by(|a, b| a.serial.cmp(&b.serial));
    Ok(displays)
}

#[cfg(test)]
mod tests;
