use std::collections::HashMap;

use crate::store::Settings;

/// Turns whatever was typed into the URL bar into something loadable:
/// full URIs pass through, things that look like hosts get `http://`,
/// everything else becomes a search.
pub fn resolve(input: &str, settings: &Settings) -> String {
    let s = input.trim();
    if s.contains("://") {
        s.to_string()
    } else if s.contains(' ') || (!s.contains('.') && !s.starts_with("localhost")) {
        settings.search_url(s)
    } else {
        format!("http://{s}")
    }
}

/// Splits `scheme://host/...` into its origin (`scheme://host`) and host.
pub fn origin(url: &str) -> Option<(&str, &str)> {
    let sep = url.find("://")?;
    let rest = &url[sep + 3..];
    let host = &rest[..rest.find('/').unwrap_or(rest.len())];
    if host.is_empty() {
        return None;
    }
    Some((&url[..sep + 3 + host.len()], host))
}

pub fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |j: usize| bytes.get(j).and_then(|&b| (b as char).to_digit(16));
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                if let (Some(hi), Some(lo)) = (hex(i + 1), hex(i + 2)) {
                    out.push((hi * 16 + lo) as u8);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (decode(k), decode(v)))
        .collect()
}
