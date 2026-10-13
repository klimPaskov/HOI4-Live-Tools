use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
};
fn server_binary() -> std::path::PathBuf {
    std::env::var_os("HOI4_NATIVE_FIXTURE_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_hoi4-live-native").into())
}
#[test]
fn stdio_is_json_and_notifications_have_no_reply() {
    let mut child = Command::new(server_binary())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        for line in ["{bad".to_owned(),json!({"jsonrpc":"2.0","id":1,"method":"initialize"}).to_string(),json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string(),json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"hoi4_send_command","arguments":{"force":1}}}).to_string(),json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"hoi4_overview","arguments":{}}}).to_string()] {writeln!(input,"{line}").unwrap();}
    }
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let replies: Vec<Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(replies.len(), 5);
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert_eq!(replies[1]["id"], 1);
    let names: Vec<_> = replies[2]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "hoi4_capabilities",
            "hoi4_overview",
            "hoi4_input_geometry",
            "hoi4_technology_list",
            "hoi4_wait_for"
        ]
    );
    assert_eq!(replies[3]["result"]["isError"], true);
    assert_eq!(replies[4]["result"]["isError"], true);
}

#[test]
fn invalid_catalog_query_is_refused_before_process_access() {
    use hoi4_live_native::protocol::Server;
    let server = Server {
        process: None,
        unavailable: None,
    };
    for arguments in [
        json!({"filter":true}),
        json!({"offset":-1}),
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"offset":8193}),
        json!({"filter":"x".repeat(129)}),
        json!({"unknown":1}),
    ] {
        let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_technology_list","arguments":arguments}})).unwrap();
        assert_eq!(reply["result"]["isError"], true);
        let value: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(value["error"]["code"], "invalid_arguments");
    }
}
#[test]
fn oversized_request_recovers_at_next_line() {
    let mut child = Command::new(server_binary())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        input.write_all(&vec![b'x'; (1 << 20) + 20]).unwrap();
        input
            .write_all(b"\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n")
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let lines: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error"]["code"], -32600);
    assert_eq!(lines[1]["id"], 7);
}
#[test]
fn invalid_wait_is_refused_before_process_access() {
    use hoi4_live_native::protocol::Server;
    let server = Server {
        process: None,
        unavailable: None,
    };
    for arguments in [
        json!({"timeout_seconds":true}),
        json!({"timeout_seconds":51}),
        json!({"until_date":[1936,2,29]}),
        json!({"until_date":[1936,-1,1]}),
        json!({"until_date":[1936,1.1,1]}),
        json!({"unknown":1}),
    ] {
        let result=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_wait_for","arguments":arguments}})).unwrap();
        assert_eq!(result["result"]["isError"], true);
        let value: Value =
            serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_ne!(value["error"]["code"], "capability_unavailable");
    }
}
