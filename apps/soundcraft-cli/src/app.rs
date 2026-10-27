use super::args::arg_value;
use super::error::fail;
use serde_json::{Value, json};
use soundcraft_automation::{Backend, Remote};
use std::io::Write;
use std::process::ExitCode;

/// Control-channel methods. Every other dotted id is an engine command.
fn is_control_method(method: &str) -> bool {
    matches!(
        method,
        "engine.execute"
            | "command"
            | "engine.commands"
            | "engine.parity"
            | "session.inspect"
            | "document.inspect"
            | "ui.inspect"
            | "ui.menu.list"
            | "ui.menu.invoke"
            | "ui.set"
            | "ui.move"
            | "ui.click"
            | "ui.drag"
            | "ui.key"
            | "ui.text"
            | "ui.screenshot"
            | "app.quit"
    )
}

/// A command id is shorthand for `engine.execute`. Documented control methods stay methods.
fn app_request(method: &str, params: Value) -> (String, Value) {
    if is_control_method(method) || !method.contains('.') {
        (method.to_string(), params)
    } else {
        ("engine.execute".to_string(), json!({"command": method, "params": params}))
    }
}

pub(crate) fn app(args: &[String]) -> ExitCode {
    let port = arg_value(args, "--port").or_else(|| std::env::var("SOUNDCRAFT_CONTROL_PORT").ok()).unwrap_or_else(|| "7979".into());
    let positional: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !(a.starts_with("--") || (*i > 0 && args.get(i - 1).is_some_and(|p| p == "--port"))))
        .map(|(_, a)| a)
        .collect();
    let Some(method) = positional.first() else { return fail("app [--port P] METHOD [JSON]") };
    let params = match positional.get(1) {
        Some(p) => match serde_json::from_str::<Value>(p) {
            Ok(v) => v,
            Err(err) => return fail(format!("{method}: bad JSON: {err}")),
        },
        None => json!({}),
    };
    let mut r = Remote::new(&port);
    // A bare command id is shorthand for engine.execute. Control-channel methods stay methods.
    let (m, p) = app_request(method, params);
    match r.call(&m, p) {
        Ok(v) => {
            outln!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_ids_are_execute_shorthand_and_control_methods_stay_methods() {
        let (m, p) = app_request("engine.history", json!({}));
        assert_eq!(m, "engine.execute");
        assert_eq!(p["command"], "engine.history");
        let (m, p) = app_request("ui.theme", json!({"mode": "light"}));
        assert_eq!(m, "engine.execute");
        assert_eq!(p["params"]["mode"], "light");
        let (m, _) = app_request("session.new", json!({"name": "Session A"}));
        assert_eq!(m, "engine.execute");
        let (m, _) = app_request("track.new", json!({}));
        assert_eq!(m, "engine.execute");
        for method in ["session.inspect", "engine.commands", "engine.parity", "ui.screenshot", "app.quit"] {
            assert_eq!(app_request(method, json!({})).0, method);
        }
    }
}
