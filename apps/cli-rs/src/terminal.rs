use std::io::{Read, Write};
use std::time::Duration;
use tokio::time::timeout;

const FALLBACK_COLOR: (u8, u8, u8) = (0, 0, 0);

pub async fn detect_terminal_background() -> (u8, u8, u8) {
    match timeout(Duration::from_millis(300), detect_bg_inner()).await {
        Ok(color) => color,
        Err(_) => FALLBACK_COLOR,
    }
}

async fn detect_bg_inner() -> (u8, u8, u8) {
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(b"\x1b]11;?\x1b\\");
    let _ = stdout.flush();

    let mut buf = [0u8; 64];
    let mut stdin = std::io::stdin();

    match timeout(
        Duration::from_millis(300),
        tokio::task::spawn_blocking(move || {
            stdin
                .read(&mut buf)
                .ok()
                .map(|n| String::from_utf8_lossy(&buf[..n]).to_string())
        }),
    )
    .await
    {
        Ok(Ok(Some(response))) => parse_osc11_response(&response),
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
