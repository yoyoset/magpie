//! `magpie mcp`: a Model Context Protocol server over stdio.
//!
//! Lets Claude (or any MCP client) read and write the plan directly: capture a
//! todo or idea the moment it comes up in conversation, look at today, plan
//! tomorrow. Hand-rolled on purpose: the protocol surface needed here is four
//! JSON-RPC methods, and the CLI stays synchronous and dependency-light.
//!
//! Transport rules: one JSON-RPC message per line on stdin/stdout; stdout
//! carries protocol only, diagnostics go to stderr.

mod tools;

use std::io::{BufRead, Write};

use serde_json::{json, Value};

pub use tools::Tools;

/// Protocol versions we speak, newest first. We echo the client's if we know it.
const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Magpie 是用户的待办、点子池和提醒中心。对话里出现「需要用户去做的事」「以后可能做的点子」\
「某个安排什么时候需要用户配合」时，立刻用 add_todo / add_idea 记下来，并写清 why（为什么）和 action（用户具体要做什么），\
标题用动词开头。不确定哪天做的，用 add_idea 放进点子池，不要硬排日期。用户问今天/明天做什么时用 today / plan。";

/// Something that can execute tools. The real one talks to the Magpie API.
pub trait ToolExecutor {
    fn list(&self) -> Value;
    /// `Ok(text)` for a result, `Err(text)` for a tool-level error the model should see.
    fn call(&self, name: &str, args: &Value) -> Result<String, String>;
}

fn response(id: &Value, result: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Handles one message. `None` for notifications (no reply expected).
pub fn handle(msg: &Value, tools: &dyn ToolExecutor) -> Option<Value> {
    let method = msg["method"].as_str().unwrap_or_default();
    let Some(id) = msg.get("id").filter(|id| !id.is_null()) else {
        return None; // notification, e.g. notifications/initialized
    };
    Some(match method {
        "initialize" => {
            let asked = msg["params"]["protocolVersion"]
                .as_str()
                .unwrap_or_default();
            let version = SUPPORTED_VERSIONS
                .iter()
                .find(|v| **v == asked)
                .copied()
                .unwrap_or(SUPPORTED_VERSIONS[0]);
            response(
                id,
                &json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "magpie", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => response(id, &json!({})),
        "tools/list" => response(id, &json!({ "tools": tools.list() })),
        "tools/call" => {
            let name = msg["params"]["name"].as_str().unwrap_or_default();
            let args = msg["params"].get("arguments").cloned().unwrap_or(json!({}));
            let (text, is_error) = match tools.call(name, &args) {
                Ok(t) => (t, false),
                Err(t) => (t, true),
            };
            response(
                id,
                &json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }),
            )
        }
        _ => error(id, -32601, &format!("method not found: {method}")),
    })
}

/// Serves until stdin closes.
pub fn serve(tools: &dyn ToolExecutor) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&msg, tools),
            Err(e) => Some(error(&Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(r) = reply {
            // serde_json never emits raw newlines inside a value, so one line per message holds.
            writeln!(stdout, "{r}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl ToolExecutor for Fake {
        fn list(&self) -> Value {
            json!([{ "name": "today", "inputSchema": { "type": "object" } }])
        }
        fn call(&self, name: &str, _: &Value) -> Result<String, String> {
            if name == "today" {
                Ok("2 items".into())
            } else {
                Err(format!("unknown tool {name}"))
            }
        }
    }

    #[test]
    fn initialize_echoes_known_version_and_offers_tools() {
        let r = handle(
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}),
            &Fake,
        )
        .unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert_eq!(r["result"]["serverInfo"]["name"], "magpie");

        let r = handle(
            &json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}),
            &Fake,
        )
        .unwrap();
        assert_eq!(r["result"]["protocolVersion"], SUPPORTED_VERSIONS[0]);
    }

    #[test]
    fn notifications_get_no_reply() {
        assert!(handle(
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            &Fake
        )
        .is_none());
    }

    #[test]
    fn tool_results_and_tool_errors_are_both_results() {
        let ok = handle(
            &json!({"jsonrpc":"2.0","id":"a","method":"tools/call","params":{"name":"today","arguments":{}}}),
            &Fake,
        )
        .unwrap();
        assert_eq!(ok["id"], "a");
        assert_eq!(ok["result"]["content"][0]["text"], "2 items");
        assert_eq!(ok["result"]["isError"], false);

        let bad = handle(
            &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope"}}),
            &Fake,
        )
        .unwrap();
        assert_eq!(
            bad["result"]["isError"], true,
            "tool errors go to the model, not JSON-RPC errors"
        );
    }

    #[test]
    fn unknown_method_is_a_jsonrpc_error() {
        let r = handle(
            &json!({"jsonrpc":"2.0","id":4,"method":"resources/list"}),
            &Fake,
        )
        .unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }
}
