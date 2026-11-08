use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
pub const LIMIT: usize = 1 << 20;
pub fn read<R: BufRead>(input: &mut R) -> io::Result<Option<std::result::Result<Vec<u8>, ()>>> {
    let mut line = Vec::new();
    let mut too_large = false;
    loop {
        let bytes = input.fill_buf()?;
        if bytes.is_empty() {
            break;
        }
        let end = bytes
            .iter()
            .position(|v| *v == b'\n')
            .map_or(bytes.len(), |v| v + 1);
        if line.len() + end <= LIMIT {
            line.extend_from_slice(&bytes[..end]);
        } else {
            too_large = true;
        }
        let complete = bytes[end - 1] == b'\n';
        input.consume(end);
        if complete {
            break;
        }
    }
    if line.is_empty() && !too_large {
        Ok(None)
    } else {
        Ok(Some(if too_large { Err(()) } else { Ok(line) }))
    }
}
pub fn serve<R: BufRead, W: Write>(
    mut input: R,
    mut output: W,
    mut handler: impl FnMut(Value) -> Option<Value>,
) -> io::Result<()> {
    while let Some(line) = read(&mut input)? {
        let reply = match line {
            Err(()) => Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Request exceeds bound"}}),
            ),
            Ok(line) => match serde_json::from_slice(&line) {
                Ok(message) => handler(message),
                Err(_) => Some(
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
                ),
            },
        };
        if let Some(reply) = reply {
            serde_json::to_writer(&mut output, &reply)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// Tools that may run on the read lane: listed tools whose schema takes no input lease (every
/// input tool requires one), except those that change server state.
pub fn read_tools(listed: Option<&Value>) -> std::collections::HashSet<String> {
    const SERIAL: [&str; 5] = [
        "hoi4_input_lease",
        "hoi4_plan",
        "hoi4_plan_run",
        "hoi4_daily_governor",
        "hoi4_wait_for",
    ];
    listed
        .and_then(|v| v["result"]["tools"].as_array())
        .map(|tools| {
            tools
                .iter()
                .filter(|t| t["inputSchema"]["properties"].get("lease_token").is_none())
                .filter_map(|t| t["name"].as_str())
                .filter(|n| !SERIAL.contains(n))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
/// A `tools/call` of a read-lane tool without a request id (an id marks an action receipt).
pub fn is_read_call(message: &Value, reads: &std::collections::HashSet<String>) -> bool {
    message["method"] == "tools/call"
        && message["params"]["arguments"].get("request_id").is_none()
        && message["params"]["name"]
            .as_str()
            .is_some_and(|n| reads.contains(n))
}

/// Two lanes over one stdio connection (JSON-RPC allows out-of-order replies by id).
/// Messages for which `is_read` holds run on their own threads at once (at most `max_reads`
/// at a time; beyond that they run inline on the reading thread); every other message goes,
/// in arrival order, to one worker thread. A memory read therefore never waits behind an
/// input action or a plan run, and actions keep their order and their single input owner.
pub fn serve_lanes<R: BufRead, W: Write + Send>(
    input: R,
    output: W,
    is_read: impl Fn(&Value) -> bool + Sync,
    handler: impl Fn(Value) -> Option<Value> + Sync,
    max_reads: usize,
) -> io::Result<()> {
    serve_lanes_idle(input, output, is_read, handler, max_reads, || false)
}

/// `serve_lanes` with background work for the input lane: whenever no message waits, the
/// worker calls `idle` (one bounded step, e.g. one autopilot action; true when it did work).
/// A queued message always goes first, so it waits at most for the step in progress.
pub fn serve_lanes_idle<R: BufRead, W: Write + Send>(
    mut input: R,
    output: W,
    is_read: impl Fn(&Value) -> bool + Sync,
    handler: impl Fn(Value) -> Option<Value> + Sync,
    max_reads: usize,
    idle: impl Fn() -> bool + Sync,
) -> io::Result<()> {
    use std::sync::{atomic::AtomicUsize, atomic::Ordering, mpsc, Mutex};
    let output = Mutex::new(output);
    let write = |reply: Value| -> io::Result<()> {
        let mut out = output
            .lock()
            .map_err(|_| io::Error::other("output lock poisoned"))?;
        serde_json::to_writer(&mut *out, &reply)?;
        out.write_all(b"\n")?;
        out.flush()
    };
    let running = AtomicUsize::new(0);
    let (to_worker, worker_in) = mpsc::channel::<Value>();
    std::thread::scope(|scope| -> io::Result<()> {
        let write = &write;
        let handler = &handler;
        let idle = &idle;
        let worker = scope.spawn(move || -> io::Result<()> {
            use std::sync::mpsc::{RecvTimeoutError, TryRecvError};
            loop {
                let message = match worker_in.try_recv() {
                    Ok(m) => m,
                    Err(TryRecvError::Disconnected) => return Ok(()),
                    Err(TryRecvError::Empty) => {
                        if idle() {
                            continue;
                        }
                        match worker_in.recv_timeout(std::time::Duration::from_millis(200)) {
                            Ok(m) => m,
                            Err(RecvTimeoutError::Timeout) => continue,
                            Err(RecvTimeoutError::Disconnected) => return Ok(()),
                        }
                    }
                };
                if let Some(reply) = handler(message) {
                    write(reply)?;
                }
            }
        });
        let result = (|| -> io::Result<()> {
            while let Some(line) = read(&mut input)? {
                let message = match line {
                    Err(()) => {
                        write(
                            json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Request exceeds bound"}}),
                        )?;
                        continue;
                    }
                    Ok(line) => match serde_json::from_slice::<Value>(&line) {
                        Ok(message) => message,
                        Err(_) => {
                            write(
                                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
                            )?;
                            continue;
                        }
                    },
                };
                if is_read(&message) {
                    if running.fetch_add(1, Ordering::SeqCst) < max_reads {
                        let running = &running;
                        scope.spawn(move || {
                            if let Some(reply) = handler(message) {
                                let _ = write(reply);
                            }
                            running.fetch_sub(1, Ordering::SeqCst);
                        });
                    } else {
                        running.fetch_sub(1, Ordering::SeqCst);
                        if let Some(reply) = handler(message) {
                            write(reply)?;
                        }
                    }
                } else if to_worker.send(message).is_err() {
                    return Err(io::Error::other("input worker stopped"));
                }
            }
            Ok(())
        })();
        drop(to_worker);
        let worked = worker
            .join()
            .map_err(|_| io::Error::other("input worker panicked"))?;
        result.and(worked)
    })
}

#[cfg(test)]
mod lane_tests {
    use super::*;
    use std::time::{Duration, Instant};
    fn request(id: u64, name: &str) -> String {
        format!(
            "{}\n",
            json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name}})
        )
    }
    fn run(lines: &str, slow_ms: u64) -> (Vec<(u64, Instant)>, Instant) {
        let start = Instant::now();
        let out = std::sync::Mutex::new(Vec::<(u64, Instant)>::new());
        struct Sink<'a>(&'a std::sync::Mutex<Vec<(u64, Instant)>>, Vec<u8>);
        impl Write for Sink<'_> {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                self.1.extend_from_slice(b);
                while let Some(end) = self.1.iter().position(|c| *c == b'\n') {
                    let line: Vec<u8> = self.1.drain(..=end).collect();
                    let v: Value = serde_json::from_slice(&line[..end]).unwrap();
                    self.0
                        .lock()
                        .unwrap()
                        .push((v["id"].as_u64().unwrap(), Instant::now()));
                }
                Ok(b.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        serve_lanes(
            io::Cursor::new(lines.as_bytes().to_vec()),
            Sink(&out, Vec::new()),
            |m| m["params"]["name"] == "read",
            |m| {
                if m["params"]["name"] == "act" {
                    std::thread::sleep(Duration::from_millis(slow_ms));
                }
                Some(json!({"jsonrpc":"2.0","id":m["id"],"result":{}}))
            },
            4,
        )
        .unwrap();
        (out.into_inner().unwrap(), start)
    }
    #[test]
    fn only_lease_free_tools_without_request_ids_are_reads() {
        let listed = json!({"result":{"tools":[
            {"name":"hoi4_armies","inputSchema":{"properties":{}}},
            {"name":"hoi4_army_move","inputSchema":{"properties":{"lease_token":{}}}},
            {"name":"hoi4_plan","inputSchema":{"properties":{}}}]}});
        let reads = read_tools(Some(&listed));
        let call = |name: &str, args: Value| json!({"method":"tools/call","params":{"name":name,"arguments":args}});
        assert!(is_read_call(&call("hoi4_armies", json!({})), &reads));
        assert!(!is_read_call(
            &call("hoi4_armies", json!({"request_id":"x"})),
            &reads
        ));
        assert!(!is_read_call(&call("hoi4_army_move", json!({})), &reads));
        assert!(!is_read_call(&call("hoi4_plan", json!({})), &reads));
        assert!(!is_read_call(&json!({"method":"tools/list"}), &reads));
    }
    #[test]
    fn idle_work_runs_between_messages_and_never_ahead_of_one() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let steps = AtomicUsize::new(0);
        let order = std::sync::Mutex::new(Vec::<&str>::new());
        // the reader pauses so the worker is idle for a while before the action arrives
        struct Slow(Vec<u8>, usize);
        impl io::Read for Slow {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.1 == 0 {
                    std::thread::sleep(Duration::from_millis(150));
                }
                let n = (self.0.len() - self.1).min(buf.len());
                buf[..n].copy_from_slice(&self.0[self.1..self.1 + n]);
                self.1 += n;
                Ok(n)
            }
        }
        let input = io::BufReader::new(Slow(request(1, "act").into_bytes(), 0));
        serve_lanes_idle(
            input,
            io::sink(),
            |_| false,
            |m| {
                order.lock().unwrap().push("act");
                Some(json!({"jsonrpc":"2.0","id":m["id"],"result":{}}))
            },
            4,
            || {
                if steps.fetch_add(1, Ordering::SeqCst) < 3 {
                    order.lock().unwrap().push("idle");
                    std::thread::sleep(Duration::from_millis(20));
                    true
                } else {
                    false
                }
            },
        )
        .unwrap();
        let order = order.into_inner().unwrap();
        assert!(order.contains(&"act"));
        assert!(order.iter().filter(|o| **o == "idle").count() >= 1);
    }

    #[test]
    fn a_read_returns_while_an_action_runs() {
        let lines = request(1, "act") + &request(2, "read");
        // Wide margins: the read must answer well before the slow action ends, even on a
        // loaded CI runner.
        let (out, start) = run(&lines, 1000);
        let at = |id| out.iter().find(|(i, _)| *i == id).unwrap().1 - start;
        assert!(
            at(2) < Duration::from_millis(600),
            "read waited {:?}",
            at(2)
        );
        assert!(at(1) >= Duration::from_millis(1000));
    }
    #[test]
    fn actions_keep_their_order_and_every_request_gets_one_reply() {
        let lines: String = (1..=6)
            .map(|i| request(i, if i % 2 == 0 { "read" } else { "act" }))
            .collect();
        let (out, _) = run(&lines, 20);
        let mut ids: Vec<u64> = out.iter().map(|(i, _)| *i).collect();
        let acts: Vec<u64> = ids.iter().copied().filter(|i| i % 2 == 1).collect();
        assert_eq!(acts, vec![1, 3, 5]);
        ids.sort();
        assert_eq!(ids, vec![1, 2, 3, 4, 5, 6]);
    }
}
