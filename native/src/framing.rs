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
