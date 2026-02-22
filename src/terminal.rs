use std::io::{Read, Write};

const FALLBACK_COLOR: (u8, u8, u8) = (0, 0, 0);

pub fn detect_terminal_background_sync() -> (u8, u8, u8) {
    let mut stdout = std::io::stdout();
    if stdout.write_all(b"\x1b]11;?\x1b\\").is_err() || stdout.flush().is_err() {
        return FALLBACK_COLOR;
    }

    let mut pollfd = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };

    let ready = unsafe { libc::poll(&mut pollfd, 1, 300) };
    if ready <= 0 {
        return FALLBACK_COLOR;
    }

    let mut buf = [0u8; 64];
    let mut stdin = std::io::stdin();
    match stdin.read(&mut buf) {
        Ok(n) if n > 0 => {
            let response = String::from_utf8_lossy(&buf[..n]);
            parse_osc11_response(&response)
        }
        _ => FALLBACK_COLOR,
    }
}

fn parse_osc11_response(response: &str) -> (u8, u8, u8) {
    // Response format: ...\]11;rgb:RRRR/GGGG/BBBB...
    if let Some(idx) = response.find("]11;rgb:") {
        let rest = &response[idx + 8..];
        let parts: Vec<&str> = rest.splitn(4, '/').collect();
        if parts.len() >= 3 {
            let b_str = parts[2]
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .collect::<String>();
            let r = parse_hex_first2(parts[0]);
            let g = parse_hex_first2(parts[1]);
            let b = parse_hex_first2(&b_str);
            return (r, g, b);
        }
    }
    FALLBACK_COLOR
}

fn parse_hex_first2(s: &str) -> u8 {
    let s = if s.len() >= 2 { &s[..2] } else { s };
    u8::from_str_radix(s, 16).unwrap_or(0)
}
