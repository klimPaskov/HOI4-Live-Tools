use crate::{
    calendar,
    error::{Error, Result},
    game,
    platform::Process,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
pub struct Server {
    pub process: Option<Box<dyn Process + Send + Sync>>,
    pub unavailable: Option<Error>,
}
fn tools() -> Value {
    json!({"tools":[
        {"name":"hoi4_capabilities","description":"Verified runtime capabilities and explicit platform/build limitations.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"hoi4_overview","description":"Fresh owned-country process snapshot. Pure bounded reads; never pauses.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"hoi4_wait_for","description":"Bounded server wait for own focus/research change or game date. Never pauses.","inputSchema":{"type":"object","properties":{"timeout_seconds":{"type":"integer","minimum":1,"maximum":50},"until_date":{"type":"array","items":{"type":"integer"},"minItems":3,"maxItems":3}},"additionalProperties":false}}
    ]})
}
fn content(value: Value) -> Value {
    json!({"content":[{"type":"text","text":serde_json::to_string(&value).unwrap()}]})
}
impl Server {
    fn overview(&self) -> Result<Value> {
        let p = self.process.as_ref().ok_or_else(|| {
            Error::new(
                "capability_unavailable",
                self.unavailable
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "No owned session manifest; use --session".into()),
            )
        })?;
        game::snapshot(p.as_ref())
    }
    fn call(&self, name: &str, args: &Value) -> Result<Value> {
        let object = args
            .as_object()
            .ok_or_else(|| Error::new("invalid_arguments", "Tool arguments must be an object"))?;
        match name {
            "hoi4_capabilities" if object.is_empty() => Ok(
                json!({"runtime":"rust","platform":std::env::consts::OS,"owned_session_bound":self.process.is_some(),"unavailable":self.unavailable,"observations":["own_focus","own_research","clock","bounded_wait"],"commands":"unavailable: native transactions not yet ported","default_mode":"FAIR","mod_qa":"separate; unavailable in this runtime","windows_macos":"unverified native adapters"}),
            ),
            "hoi4_overview" if object.is_empty() => self.overview(),
            "hoi4_wait_for" => {
                if object
                    .keys()
                    .any(|k| k != "timeout_seconds" && k != "until_date")
                {
                    return Err(Error::new("invalid_arguments", "Unknown wait argument"));
                }
                let timeout = if let Some(v) = object.get("timeout_seconds") {
                    v.as_u64()
                        .ok_or_else(|| Error::new("invalid_arguments", "Timeout must be integer"))?
                } else {
                    45
                };
                if !(1..=50).contains(&timeout) {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Timeout must be 1..50 seconds",
                    ));
                }
                let target = if let Some(v) = object.get("until_date") {
                    let v = v.as_array().filter(|a| a.len() == 3).ok_or_else(|| {
                        Error::new(
                            "invalid_arguments",
                            "until_date must contain year,month,day",
                        )
                    })?;
                    let numbers: Vec<_> = v
                        .iter()
                        .map(|n| {
                            n.as_i64().ok_or_else(|| {
                                Error::new("invalid_arguments", "Date components must be integers")
                            })
                        })
                        .collect::<Result<_>>()?;
                    Some(calendar::encode(
                        numbers[0],
                        usize::try_from(numbers[1])
                            .map_err(|_| Error::new("invalid_date", "Invalid month"))?,
                        numbers[2],
                    )?)
                } else {
                    None
                };
                let before = self.overview()?;
                let mut current = before.clone();
                let start = Instant::now();
                loop {
                    let mut reasons = Vec::new();
                    for field in ["focus", "research"] {
                        if current[field] != before[field] {
                            reasons.push(format!("{field}_changed"));
                        }
                    }
                    if target.is_some_and(|v| {
                        current["date"]["raw"]
                            .as_i64()
                            .is_some_and(|n| n >= i64::from(v))
                    }) {
                        reasons.push("date_reached".into());
                    }
                    if !reasons.is_empty() || start.elapsed() >= Duration::from_secs(timeout) {
                        if reasons.is_empty() {
                            reasons.push("wall_timeout".into());
                        }
                        return Ok(
                            json!({"reasons":reasons,"state":current,"elapsed_seconds":start.elapsed().as_secs_f64()}),
                        );
                    }
                    std::thread::sleep(Duration::from_millis(250));
                    current = self.overview()?;
                }
            }
            _ => Err(Error::new(
                "capability_unavailable",
                "Unknown tool or unsupported arguments; no raw/debug/action fallback",
            )),
        }
    }
    pub fn handle(&self, msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let valid = msg.is_object()
            && msg.get("jsonrpc") == Some(&json!("2.0"))
            && msg.get("method").and_then(Value::as_str).is_some();
        if !valid {
            return Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid Request"}}),
            );
        }
        let method = msg["method"].as_str().unwrap();
        id.as_ref()?;
        let result = match method {
            "initialize" => Ok(
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"hoi4-live-native","version":env!("CARGO_PKG_VERSION")}}),
            ),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(tools()),
            "tools/call" => {
                let name = msg["params"]["name"].as_str().unwrap_or("");
                let empty = json!({});
                let args = msg["params"].get("arguments").unwrap_or(&empty);
                Ok(match self.call(name, args) {
                    Ok(v) => content(v),
                    Err(e) => {
                        let mut v = content(json!({"error":e}));
                        v["isError"] = json!(true);
                        v
                    }
                })
            }
            _ => Err((-32601, "Method not found")),
        };
        Some(match result {
            Ok(v) => json!({"jsonrpc":"2.0","id":id,"result":v}),
            Err((code, message)) => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
            }
        })
    }
}
