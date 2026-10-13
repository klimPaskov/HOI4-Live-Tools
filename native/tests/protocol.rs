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
            "hoi4_research",
            "hoi4_input_geometry",
            "hoi4_research_ui",
            "hoi4_research_nodes",
            "hoi4_focus_list",
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
fn invalid_research_query_is_refused_before_process_access() {
    use hoi4_live_native::protocol::Server;
    let server = Server {
        process: None,
        unavailable: None,
    };
    for arguments in [
        json!({"offset":-1}),
        json!({"offset":8193}),
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"limit":true}),
        json!({"country":"FRA"}),
    ] {
        let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_research","arguments":arguments}})).unwrap();
        let value: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(value["error"]["code"], "invalid_arguments");
    }
}
#[test]
fn research_ui_refuses_selector_or_action_arguments_before_process_access() {
    use hoi4_live_native::protocol::Server;
    let server = Server {
        process: None,
        unavailable: None,
    };
    for arguments in [
        json!({"country":"FRA"}),
        json!({"click":true}),
        json!({"slot":0}),
    ] {
        let reply = server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_research_ui","arguments":arguments}})).unwrap();
        let v: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(v["error"]["code"], "invalid_arguments");
    }
}

#[test]
fn own_focus_and_node_queries_refuse_invalid_arguments_before_process_access() {
    use hoi4_live_native::protocol::Server;
    let server = Server {
        process: None,
        unavailable: None,
    };
    for (name, arguments) in [
        ("hoi4_focus_list", json!({"country":"FRA"})),
        ("hoi4_focus_list", json!({"offset":4097})),
        ("hoi4_focus_list", json!({"limit":101})),
        ("hoi4_focus_list", json!({"limit":true})),
        ("hoi4_focus_list", json!({"filter":null})),
        ("hoi4_focus_list", json!({"filter":"x".repeat(129)})),
        ("hoi4_focus_list", json!({"filter":"GER;start"})),
        ("hoi4_research_nodes", json!({"click":true})),
        ("hoi4_research_nodes", json!({"country":"FRA"})),
        ("hoi4_research_nodes", json!({"offset":513})),
        ("hoi4_research_nodes", json!({"limit":0})),
        ("hoi4_research_nodes", json!({"limit":101})),
        ("hoi4_research_nodes", json!({"limit":true})),
        ("hoi4_research_nodes", json!({"filter":null})),
        ("hoi4_research_nodes", json!({"filter":"bad;key"})),
        ("hoi4_research_nodes", json!({"filter":"x".repeat(129)})),
        ("hoi4_research_nodes", json!({"slot":0})),
    ] {
        let reply = server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}})).unwrap();
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

#[test]
fn invalid_ui_wait_is_refused_before_process_access() {
    use hoi4_live_native::protocol::Server;
    let server = Server {
        process: None,
        unavailable: None,
    };
    for arguments in [
        json!({"until_research_ui":null}),
        json!({"until_research_ui":{}}),
        json!({"until_research_ui":{"view":"foreign"}}),
        json!({"until_research_ui":{"view":"tree","selected_slot":null}}),
        json!({"until_research_ui":{"view":"panel","selected_slot":32}}),
        json!({"until_research_ui":{"view":"tree","selected_slot":-1}}),
        json!({"until_research_ui":{"view":"tree","selected_slot":true}}),
        json!({"until_research_ui":{"view":"tree","selected_slot":1.0}}),
        json!({"until_research_ui":{"view":"tree","click":true}}),
        json!({"until_research_ui":{"view":"tree"},"until_date":[1936,1,2]}),
    ] {
        let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_wait_for","arguments":arguments}})).unwrap();
        assert_eq!(reply["result"]["isError"], true);
        let value: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(value["error"]["code"], "invalid_arguments");
    }
}
