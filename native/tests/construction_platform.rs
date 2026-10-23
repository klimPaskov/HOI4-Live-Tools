use hoi4_live_native::{
    construction::{self, controls, find_view},
    error::{Error, Result},
    memory::{ReadMemory, StringAbi},
    platform::{Identity, Process},
    protocol::Server,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

// Reject every process operation so an unported reader cannot accidentally
// inspect either platform's payload, even through protocol recovery or batching.
struct UnavailableProcess {
    abi: StringAbi,
    accesses: Arc<AtomicUsize>,
}
impl UnavailableProcess {
    fn new(abi: StringAbi) -> Self {
        Self {
            abi,
            accesses: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn accessed(&self) -> Error {
        self.accesses.fetch_add(1, Ordering::SeqCst);
        Error::new("fixture_access", "Unreviewed fixture process access")
    }
    fn assert_untouched(&self) {
        assert_eq!(self.accesses.load(Ordering::SeqCst), 0);
    }
}
impl ReadMemory for UnavailableProcess {
    fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
        Err(self.accessed())
    }
    fn read_many(&self, _: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        Err(self.accessed())
    }
    fn string_abi(&self) -> StringAbi {
        self.abi
    }
}
impl Process for UnavailableProcess {
    fn check(&self) -> Result<()> {
        Err(self.accessed())
    }
    fn begin_snapshot(&self) -> Result<()> {
        Err(self.accessed())
    }
    fn image_base(&self) -> u64 {
        let _ = self.accessed();
        0
    }
    fn identity(&self) -> Option<&Identity> {
        let _ = self.accessed();
        None
    }
}

#[test]
fn mac_controls_refuse_before_snapshot_identity_or_reads() {
    let process = UnavailableProcess::new(StringAbi::LibCxx);
    for geometry in [false, true] {
        assert_eq!(
            controls::snapshot(&process, "arms_factory", geometry)
                .unwrap_err()
                .code,
            "unsupported_platform"
        );
    }
    process.assert_untouched();
}

#[test]
fn mac_find_refuses_with_and_without_requested_state_before_any_process_access() {
    let process = UnavailableProcess::new(StringAbi::LibCxx);
    for state in [None, Some(1), Some(4095)] {
        assert_eq!(
            find_view::snapshot(&process, state).unwrap_err().code,
            "unsupported_platform"
        );
    }
    process.assert_untouched();
}

#[test]
fn mac_queue_ui_refuses_even_for_an_empty_requested_page() {
    let process = UnavailableProcess::new(StringAbi::LibCxx);
    for offset in [0, 2048] {
        for location in [false, true] {
            assert_eq!(
                construction::page_with_options(&process, offset, 128, location, true)
                    .unwrap_err()
                    .code,
                "unsupported_platform"
            );
        }
    }
    process.assert_untouched();
}

#[test]
fn construction_arguments_are_validated_before_platform_or_process_access() {
    for abi in [StringAbi::LibCxx, StringAbi::LibStdCxx] {
        let process = UnavailableProcess::new(abi);
        for key in ["", "bad key", &"a".repeat(129)] {
            assert_eq!(
                controls::snapshot(&process, key, true).unwrap_err().code,
                "invalid_arguments"
            );
        }
        for state in [i32::MIN, 0, 4096, i32::MAX] {
            assert_eq!(
                find_view::snapshot(&process, Some(state)).unwrap_err().code,
                "invalid_arguments"
            );
        }
        for (offset, limit) in [(2049, 1), (0, 0), (0, 129)] {
            assert_eq!(
                construction::page_with_options(&process, offset, limit, true, true)
                    .unwrap_err()
                    .code,
                "invalid_arguments"
            );
        }
        process.assert_untouched();
    }
}

#[test]
fn linux_ui_and_mac_non_ui_queue_remain_reachable() {
    let linux = UnavailableProcess::new(StringAbi::LibStdCxx);
    assert_eq!(
        controls::snapshot(&linux, "arms_factory", false)
            .unwrap_err()
            .code,
        "fixture_access"
    );
    assert_eq!(
        find_view::snapshot(&linux, None).unwrap_err().code,
        "fixture_access"
    );
    assert_eq!(
        construction::page_with_options(&linux, 0, 32, false, true)
            .unwrap_err()
            .code,
        "fixture_access"
    );
    assert_eq!(linux.accesses.load(Ordering::SeqCst), 3);
    let mac = UnavailableProcess::new(StringAbi::LibCxx);
    for location in [false, true] {
        assert_eq!(
            construction::page_with_options(&mac, 0, 32, location, false)
                .unwrap_err()
                .code,
            "fixture_access"
        );
    }
    assert_eq!(mac.accesses.load(Ordering::SeqCst), 2);
}

fn error_code(server: &Server, name: &str, args: Value) -> String {
    let response = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":name,"arguments":args}}))
        .unwrap();
    assert_eq!(response["result"]["isError"], true, "{response}");
    let body: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    body["error"]["code"].as_str().unwrap().to_string()
}

#[test]
fn public_mac_routes_preserve_explicit_platform_refusal_without_read_retries() {
    let process = UnavailableProcess::new(StringAbi::LibCxx);
    let accesses = process.accesses.clone();
    let server = Server {
        process: Some(Box::new(process)),
        unavailable: None,
    };
    for (tool, args) in [
        (
            "hoi4_construction_controls",
            json!({"building_id":"arms_factory"}),
        ),
        (
            "hoi4_construction_controls",
            json!({"building_id":"arms_factory","include_geometry":true}),
        ),
        ("hoi4_construction_find_view", json!({})),
        ("hoi4_construction_find_view", json!({"state_id":64})),
        ("hoi4_construction_queue", json!({"include_ui":true})),
        (
            "hoi4_construction_queue",
            json!({"include_ui":true,"include_location":false,"offset":2048}),
        ),
    ] {
        assert_eq!(error_code(&server, tool, args), "unsupported_platform");
    }
    for (tool, args) in [
        (
            "hoi4_construction_controls",
            json!({"building_id":"arms_factory","include_geometry":1}),
        ),
        (
            "hoi4_construction_controls",
            json!({"building_id":"bad key"}),
        ),
        (
            "hoi4_construction_controls",
            json!({"building_id":"arms_factory","click":true}),
        ),
        ("hoi4_construction_find_view", json!({"state_id":0})),
        ("hoi4_construction_find_view", json!({"state_id":1.5})),
        (
            "hoi4_construction_find_view",
            json!({"state_id":64,"country":"ENG"}),
        ),
        (
            "hoi4_construction_queue",
            json!({"include_ui":true,"limit":0}),
        ),
        ("hoi4_construction_queue", json!({"include_ui":1})),
    ] {
        assert_eq!(error_code(&server, tool, args), "invalid_arguments");
    }
    assert_eq!(accesses.load(Ordering::SeqCst), 0);
}
