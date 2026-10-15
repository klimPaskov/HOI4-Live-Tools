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
fn completed_focus_query_is_read_only_and_reports_bounded_linux_evidence() {
    use hoi4_live_native::protocol::Server;
    let server = Server {
        process: None,
        unavailable: None,
    };
    for arguments in [
        json!([]),
        json!({"offset":-1}),
        json!({"offset":4097}),
        json!({"offset":1.5}),
        json!({"offset":null}),
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"limit":null}),
        json!({"country":"OTHER"}),
        json!({"force":true}),
        json!({"filter":"GER"}),
    ] {
        let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_focus_completed","arguments":arguments}})).unwrap();
        let body: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(body["error"]["code"], "invalid_arguments", "{body}");
    }
    let list = server
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .unwrap();
    let tool = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "hoi4_focus_completed")
        .unwrap();
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    assert_eq!(tool["inputSchema"]["properties"]["offset"]["maximum"], 4096);
    let reply=server.handle(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"hoi4_capabilities","arguments":{}}})).unwrap();
    let body: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(!body["unverified_observations"]
        .as_array()
        .unwrap()
        .contains(&json!("own_focus_completion_membership")));
    assert!(
        body["bounded_observation_validation"]["own_focus_completion_membership"]
            .as_str()
            .unwrap()
            .starts_with("run052:")
    );
    assert_eq!(body["owned_session_bound"], false);
    assert_eq!(body["windows_macos"], "unverified native adapters");
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
            "hoi4_research_slot_control",
            "hoi4_research_category",
            "hoi4_research_folders",
            "hoi4_research_ui",
            "hoi4_army",
            "hoi4_economy",
            "hoi4_production_lines",
            "hoi4_research_node_detail",
            "hoi4_research_nodes",
            "hoi4_focus_list",
            "hoi4_focus_completed",
            "hoi4_focus_search_state",
            "hoi4_focus_node_detail",
            "hoi4_focus_selected_detail",
            "hoi4_technology_list",
            "hoi4_wait_for"
        ]
    );
    assert_eq!(replies[3]["result"]["isError"], true);
    assert_eq!(replies[4]["result"]["isError"], true);
}

#[test]
fn focus_search_query_is_read_only_and_reports_bounded_manual_evidence() {
    use hoi4_live_native::protocol::Server;
    let server = Server {
        process: None,
        unavailable: None,
    };
    for arguments in [
        json!({}),
        json!([]),
        json!({"focus_id":null}),
        json!({"focus_id":""}),
        json!({"focus_id":"bad/key"}),
        json!({"focus_id":"a".repeat(129)}),
        json!({"focus_id":"GER_rhineland","country":"OTHER"}),
        json!({"focus_id":"GER_rhineland","include_geometry":true}),
        json!({"focus_id":"GER_rhineland","select":true}),
        json!({"focus_id":"GER_rhineland","include_control":1}),
        json!({"focus_id":"GER_rhineland","include_control":null}),
        json!({"focus_id":"GER_rhineland","include_control":"true"}),
    ] {
        let reply = server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_focus_search_state","arguments":arguments}})).unwrap();
        let body: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(body["error"]["code"], "invalid_arguments", "{body}");
    }
    let list = server
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .unwrap();
    let tool = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "hoi4_focus_search_state")
        .unwrap();
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    assert_eq!(
        tool["inputSchema"]["properties"]["include_control"]["default"],
        false
    );
    let reply = server.handle(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"hoi4_capabilities","arguments":{}}})).unwrap();
    let body: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(!body["unverified_observations"]
        .as_array()
        .unwrap()
        .contains(&json!("own_focus_search_selection")));
    assert!(
        body["bounded_observation_validation"]["own_focus_search_selection"]
            .as_str()
            .unwrap()
            .starts_with("run068:")
    );
    assert_eq!(body["owned_session_bound"], false);
    assert_eq!(body["windows_macos"], "unverified native adapters");
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
fn node_detail_rejects_unknown_and_malformed_args_before_process_access() {
    let server = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    for args in [
        json!({}),
        json!({"technology_id":null}),
        json!({"technology_id":true}),
        json!({"technology_id":""}),
        json!({"technology_id":"bad\nkey"}),
        json!({"technology_id":"x".repeat(257)}),
        json!({"technology_id":"tools1","click":true}),
        json!({"technology_id":"tools1","country":"FRA"}),
    ] {
        let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_research_node_detail","arguments":args}})).unwrap();
        let v: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(v["error"]["code"], "invalid_arguments");
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
        json!({"until_assignments":{}}),
        json!({"until_assignments":{"focus_id":"GER_rhineland"},"until_date":[1936,1,2]}),
        json!({"until_assignments":{"focus_id":"GER_rhineland"},"until_research_ui":{"view":"panel"}}),
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

#[test]
fn category_arguments_refuse_before_process_access() {
    let server = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    for args in [
        json!({}),
        json!({"folder_id":0}),
        json!({"folder_id":""}),
        json!({"folder_id":"industry_folder","index":0}),
        json!({"folder_id":"bad\nkey"}),
        json!({"folder_id":"industry_folder","include_geometry":0}),
    ] {
        let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_research_category","arguments":args}})).unwrap();
        assert!(reply.to_string().contains("invalid_arguments"));
    }
}
#[test]
fn focus_geometry_flag_is_optional_strict_boolean_before_process_access() {
    let server = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    for args in [
        json!({"focus_id":"GER_rhineland","include_geometry":null}),
        json!({"focus_id":"GER_rhineland","include_geometry":0}),
        json!({"focus_id":"GER_rhineland","include_geometry":"true"}),
        json!({"focus_id":"GER_rhineland","include_geometry":true,"other":1}),
    ] {
        let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_focus_node_detail","arguments":args}})).unwrap();
        assert!(reply.to_string().contains("invalid_arguments"));
    }
    for args in [
        json!({"focus_id":"GER_rhineland"}),
        json!({"focus_id":"GER_rhineland","include_geometry":false}),
        json!({"focus_id":"GER_rhineland","include_geometry":true}),
    ] {
        let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_focus_node_detail","arguments":args}})).unwrap();
        assert!(!reply.to_string().contains("invalid_arguments"));
    }
}

#[test]
fn research_folders_arguments_refuse_before_any_process_access() {
    let server = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    for arguments in [
        json!({"folder_id":"industry_folder"}),
        json!({"country":"GER"}),
        json!({"filter":"industry"}),
        json!({"offset":0}),
        json!({"include_geometry":true}),
        json!({"lease_token":"x"}),
        json!({"click":true}),
        json!([]),
        json!(null),
    ] {
        let reply = server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_research_folders","arguments":arguments}})).unwrap();
        let value: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(reply["result"]["isError"], true);
        assert_eq!(value["error"]["code"], "invalid_arguments");
    }
    let reply = server.handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"hoi4_research_folders","arguments":{}}})).unwrap();
    let value: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(value["error"]["code"], "capability_unavailable");
}

#[test]
fn research_folders_tool_is_read_only_empty_arguments_and_capability_is_listed() {
    let server = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .unwrap();
    let tool = reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["name"] == "hoi4_research_folders")
        .unwrap();
    assert_eq!(tool["inputSchema"]["properties"], json!({}));
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    let reply = server.handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"hoi4_capabilities","arguments":{}}})).unwrap();
    let value: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(value["observations"]
        .as_array()
        .unwrap()
        .contains(&json!("public_research_folder_catalog")));
    assert_eq!(value["owned_session_bound"], false);
}

#[test]
fn selected_focus_detail_query_is_strict_read_only_before_process_access() {
    use hoi4_live_native::protocol::Server;
    let server = Server {
        process: None,
        unavailable: None,
    };
    for arguments in [
        json!({}),
        json!([]),
        json!({"focus_id":null}),
        json!({"focus_id":1}),
        json!({"focus_id":""}),
        json!({"focus_id":"x".repeat(129)}),
        json!({"focus_id":"GER other"}),
        json!({"focus_id":"GER_rhineland","include_geometry":1}),
        json!({"focus_id":"GER_rhineland","include_geometry":null}),
        json!({"focus_id":"GER_rhineland","include_geometry":"true"}),
        json!({"focus_id":"GER_rhineland","country":"OTHER"}),
        json!({"focus_id":"GER_rhineland","force":true}),
        json!({"focus_id":"GER_rhineland","x":1,"y":1}),
    ] {
        let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_focus_selected_detail","arguments":arguments}})).unwrap();
        assert_eq!(reply["result"]["isError"], true);
        let value: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        let code = value["error"]["code"].as_str().unwrap();
        assert!(
            ["invalid_arguments", "invalid_argument"].contains(&code),
            "{value}"
        );
    }
    let list = server
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .unwrap();
    let tool = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "hoi4_focus_selected_detail")
        .unwrap();
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["inputSchema"]["required"], json!(["focus_id"]));
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    assert_eq!(
        tool["inputSchema"]["properties"]["include_geometry"]["type"],
        "boolean"
    );
    assert_eq!(
        tool["inputSchema"]["properties"]["include_geometry"]["default"],
        false
    );
    let caps=server.handle(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"hoi4_capabilities","arguments":{}}})).unwrap();
    let value: Value =
        serde_json::from_str(caps["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(value["observations"]
        .as_array()
        .unwrap()
        .contains(&json!("own_focus_selected_detail")));
}
