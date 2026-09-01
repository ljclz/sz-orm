use std::io::{self, Read, Write};

use sz_orm_lsp::server::LspServer;

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut server = LspServer::new();

    let mut buf = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        match stdin.lock().read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                buf.push(byte[0]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&buf);
                    if let Some(len) = parse_content_length(&header) {
                        let mut body = vec![0u8; len];
                        if stdin.lock().read_exact(&mut body).is_ok() {
                            let request = String::from_utf8_lossy(&body);
                            let response = server.handle_json_rpc(&request);
                            write_response(&mut stdout, &response)?;
                        }
                    }
                    buf.clear();
                }
            }
            Err(_) => break,
        }
    }
    Ok(())
}

fn parse_content_length(header: &str) -> Option<usize> {
    for line in header.lines() {
        if let Some(val) = line.strip_prefix("Content-Length: ") {
            return val.trim().parse().ok();
        }
    }
    None
}

fn write_response(stdout: &mut io::Stdout, body: &str) -> io::Result<()> {
    write!(stdout, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    stdout.flush()
}
