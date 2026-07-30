//! Semantics-free MCP adapter over the single HostClient interface.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use saccade_host_client::HostClient;
use saccade_protocol::{ActionReceipt, ActionRequest, ObservationSnapshot};
use serde::Deserialize;
use serde_json::{json, Value};

const MCP_VERSION: &str = "2025-03-26";
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

pub fn serve(grant_path: PathBuf) -> Result<()> {
    let host = HostClient::connect(grant_path)?;
    serve_io(&host, std::io::stdin().lock(), std::io::stdout().lock())
}

fn serve_io(host: &HostClient, input: impl BufRead, mut output: impl Write) -> Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(request) => request,
            Err(error) => {
                write_rpc(&mut output, Value::Null, Err((-32700, error.to_string())))?;
                continue;
            }
        };
        let Some(id) = request.id.clone() else {
            continue;
        };
        let result = dispatch(host, request).map_err(|error| (-32000, error.to_string()));
        write_rpc(&mut output, id, result)?;
    }
    Ok(())
}

fn dispatch(host: &HostClient, request: RpcRequest) -> Result<Value> {
    if request.jsonrpc != "2.0" {
        bail!("unsupported JSON-RPC version {}", request.jsonrpc);
    }
    match request.method.as_str() {
        "initialize" => initialize(host),
        "notifications/initialized" | "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools":tools()})),
        "tools/call" => {
            let name = string(&request.params, "name")?;
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let value = call_tool(host, name, arguments)?;
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&value)?}],"structuredContent":value,"isError":false}),
            )
        }
        method => bail!("unknown JSON-RPC method {method}"),
    }
}

fn initialize(host: &HostClient) -> Result<Value> {
    let capabilities = host.call(
        NEXT_ID.fetch_add(1, Ordering::Relaxed),
        "system.capabilities",
        json!({}),
        Duration::from_secs(10),
    )?;
    let instructions = profile_instructions(&capabilities);
    Ok(json!({
        "protocolVersion":MCP_VERSION, "capabilities":{"tools":{"listChanged":false}},
        "serverInfo":{"name":"saccade-runtime","version":env!("CARGO_PKG_VERSION")},
        "instructions":instructions
    }))
}

fn profile_instructions(capabilities: &Value) -> String {
    let profile = capabilities.get("profile");
    let name = profile
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("default");
    let behavior = profile
        .and_then(|value| value.get("behavior"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let base = "Observe a tab before acting and use only returned action tokens.";
    if behavior.is_empty() {
        format!("Active Saccade Profile: {name}. {base}")
    } else {
        format!("Active Saccade Profile: {name}. User behavior: {behavior}\n{base}")
    }
}

fn call_tool(host: &HostClient, name: &str, arguments: Value) -> Result<Value> {
    let method = name
        .strip_prefix("saccade.")
        .context("tool is outside the Saccade namespace")?;
    if ![
        "system.capabilities",
        "tabs.list",
        "tabs.open",
        "web.observe",
        "web.act",
        "web.act_native",
        "web.act_soft",
        "input_policy.list",
        "input_policy.remember_native",
        "web.reflex.run",
    ]
    .contains(&method)
    {
        bail!("tool is not registered: {name}");
    }
    validate_arguments(method, &arguments)?;
    let timeout = match method {
        "web.act" | "web.act_native" | "web.act_soft" => Duration::from_secs(30),
        "web.reflex.run" => Duration::from_millis(
            arguments
                .get("timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or(30_000)
                + 10_000,
        ),
        "tabs.open" => Duration::from_secs(15),
        _ => Duration::from_secs(10),
    };
    let result = host.call(
        NEXT_ID.fetch_add(1, Ordering::Relaxed),
        method,
        arguments,
        timeout,
    )?;
    match method {
        "web.observe" => {
            serde_json::from_value::<ObservationSnapshot>(result.clone())?.validate()?
        }
        "web.act" | "web.act_native" | "web.act_soft" => {
            let receipt: ActionReceipt = serde_json::from_value(result.clone())?;
            receipt.post_action_observation.validate()?;
        }
        _ => {}
    }
    Ok(result)
}

fn validate_arguments(method: &str, value: &Value) -> Result<()> {
    if matches!(method, "web.act" | "web.act_native" | "web.act_soft") {
        serde_json::from_value::<ActionRequest>(value.clone())?.validate()?;
        return Ok(());
    }
    let object = value
        .as_object()
        .context("tool arguments must be an object")?;
    let (allowed, required): (&[&str], &[&str]) = match method {
        "system.capabilities" | "tabs.list" | "input_policy.list" => (&[], &[]),
        "tabs.open" => (&["url", "active"], &["url"]),
        "web.observe" => (&["tab_id"], &["tab_id"]),
        "input_policy.remember_native" => {
            (&["tab_id", "action_token"], &["tab_id", "action_token"])
        }
        "web.reflex.run" => (
            &["tab_id", "input_backend", "max_actions", "timeout_ms"],
            &["tab_id"],
        ),
        _ => bail!("tool has no parameter contract"),
    };
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            bail!("unexpected argument: {key}");
        }
    }
    for key in required {
        if !object.contains_key(*key) {
            bail!("missing required argument: {key}");
        }
    }
    match method {
        "tabs.open" => {
            let url = string(value, "url")?;
            if url.len() > 8192 || !(url.starts_with("http://") || url.starts_with("https://")) {
                bail!("url must use HTTP or HTTPS and stay within 8192 bytes");
            }
            if value
                .get("active")
                .is_some_and(|active| !active.is_boolean())
            {
                bail!("active must be a boolean");
            }
        }
        "web.observe" => {
            string(value, "tab_id")?;
        }
        "input_policy.remember_native" => {
            string(value, "tab_id")?;
            if string(value, "action_token")?.len() < 32 {
                bail!("action_token must be an opaque current token");
            }
        }
        "web.reflex.run" => {
            string(value, "tab_id")?;
            if value
                .get("input_backend")
                .is_some_and(|backend| !matches!(backend.as_str(), Some("native" | "soft")))
            {
                bail!("input_backend must be native or soft");
            }
            let max_actions = value
                .get("max_actions")
                .map(|number| number.as_u64().context("max_actions must be an integer"))
                .transpose()?
                .unwrap_or(500);
            if !(1..=10_000).contains(&max_actions) {
                bail!("max_actions must be between 1 and 10000");
            }
            let timeout_ms = value
                .get("timeout_ms")
                .map(|number| number.as_u64().context("timeout_ms must be an integer"))
                .transpose()?
                .unwrap_or(30_000);
            if !(1..=60_000).contains(&timeout_ms) {
                bail!("timeout_ms must be between 1 and 60000");
            }
        }
        _ => {}
    }
    Ok(())
}

fn tools() -> Vec<Value> {
    vec![
        json!({"name":"saccade.system.capabilities","description":"Read the active Profile behavior and Runtime capabilities.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"saccade.tabs.list","description":"List tabs managed by Saccade.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"saccade.tabs.open","description":"Open an HTTP or HTTPS tab managed by Saccade.","inputSchema":{"type":"object","properties":{"url":{"type":"string","minLength":1,"maxLength":8192},"active":{"type":"boolean"}},"required":["url"],"additionalProperties":false}}),
        json!({"name":"saccade.web.observe","description":"Read the latest authorized observation.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1}},"required":["tab_id"],"additionalProperties":false}}),
        json!({"name":"saccade.web.act","description":"Run one revision-bound closed loop using the Registry-selected software or native input backend.","inputSchema":{"type":"object","properties":{"browser_instance_id":{"type":"string"},"tab_id":{"type":"string"},"document_id":{"type":"string"},"basis_revision":{"type":"integer","minimum":1},"action_token":{"type":"string","minLength":32},"operation":{"enum":["click","type","select","upload"]},"payload":{"type":"object"}},"required":["browser_instance_id","tab_id","document_id","basis_revision","action_token","operation","payload"],"additionalProperties":false}}),
        json!({"name":"saccade.web.act_native","description":"Diagnostic override: run one revision-bound closed loop with native OS input.","inputSchema":{"type":"object","properties":{"browser_instance_id":{"type":"string"},"tab_id":{"type":"string"},"document_id":{"type":"string"},"basis_revision":{"type":"integer","minimum":1},"action_token":{"type":"string","minLength":32},"operation":{"enum":["click","type","select","upload"]},"payload":{"type":"object"}},"required":["browser_instance_id","tab_id","document_id","basis_revision","action_token","operation","payload"],"additionalProperties":false}}),
        json!({"name":"saccade.web.act_soft","description":"Diagnostic override: run one revision-bound click with registered software pointer input.","inputSchema":{"type":"object","properties":{"browser_instance_id":{"type":"string"},"tab_id":{"type":"string"},"document_id":{"type":"string"},"basis_revision":{"type":"integer","minimum":1},"action_token":{"type":"string","minLength":32},"operation":{"const":"click"},"payload":{"type":"object"}},"required":["browser_instance_id","tab_id","document_id","basis_revision","action_token","operation","payload"],"additionalProperties":false}}),
        json!({"name":"saccade.input_policy.list","description":"List this user's local per-page learned input-backend records.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"saccade.input_policy.remember_native","description":"Remember that the current page control should use native input on future actions.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1},"action_token":{"type":"string","minLength":32}},"required":["tab_id","action_token"],"additionalProperties":false}}),
        json!({"name":"saccade.web.reflex.run","description":"Keep a revision-bound reflex target loop local and return millisecond receipts; omit input_backend for Registry selection.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1},"input_backend":{"enum":["native","soft"]},"max_actions":{"type":"integer","minimum":1,"maximum":10000,"default":500},"timeout_ms":{"type":"integer","minimum":1,"maximum":60000,"default":30000}},"required":["tab_id"],"additionalProperties":false}}),
    ]
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .with_context(|| format!("{key} must be a non-empty string"))
}

fn write_rpc(
    output: &mut impl Write,
    id: Value,
    result: Result<Value, (i32, String)>,
) -> Result<()> {
    let response = match result {
        Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
        Err((code, message)) => {
            json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
        }
    };
    serde_json::to_writer(&mut *output, &response)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_and_first_slice_tools_are_strict() {
        let request: RpcRequest = serde_json::from_value(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        )
        .unwrap();
        assert_eq!(request.method, "initialize");
        assert_eq!(tools().len(), 10);
        assert!(serde_json::from_value::<RpcRequest>(
            json!({"jsonrpc":"2.0","id":1,"method":"ping","unexpected":true})
        )
        .is_err());
        assert!(
            validate_arguments("web.observe", &json!({"tab_id":"x","selector":"button"})).is_err()
        );
        assert!(validate_arguments(
            "tabs.open",
            &json!({"url":"https://fixture.test","active":true})
        )
        .is_ok());
        assert!(validate_arguments("tabs.open", &json!({"active":true})).is_err());
        assert!(validate_arguments("tabs.open", &json!({"url":"file:///tmp/form"})).is_err());
        assert!(validate_arguments(
            "tabs.open",
            &json!({"url":"https://fixture.test","active":"yes"})
        )
        .is_err());
        assert_eq!(
            profile_instructions(
                &json!({"profile":{"name":"focused","behavior":"Work in page order."}})
            ),
            "Active Saccade Profile: focused. User behavior: Work in page order.\nObserve a tab before acting and use only returned action tokens."
        );
    }
}
