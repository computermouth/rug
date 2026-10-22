//! Saved logins, in the layout `pass` uses: `passwords/<host>/<username>.gpg`,
//! each a gpg-encrypted file with the password on its first line, and the
//! key ids to encrypt to in `passwords/.gpg-id`.
//!
//! Listing logins only reads file names; the passphrase (asked for by
//! gpg-agent's pinentry) is needed only to fill one in. Everything that runs
//! gpg blocks, so call it from a worker thread.

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::paths;

/// File name for a login saved without a username.
const NO_USERNAME: &str = "(no username)";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Login {
    /// The directory it's saved under; a domain, or a host with a port.
    pub host: String,
    pub username: String,
}

impl Login {
    pub fn new(host: &str, username: &str) -> Self {
        Self { host: host.to_string(), username: username.to_string() }
    }

    fn path(&self) -> PathBuf {
        let name = if self.username.is_empty() { NO_USERNAME } else { &self.username };
        paths::passwords_dir().join(file_name(&self.host)).join(format!("{}.gpg", file_name(name)))
    }

    /// Whether this login belongs on a page at `host`: the same host, or a
    /// subdomain of it.
    pub fn matches(&self, host: &str) -> bool {
        host == self.host || host.ends_with(&format!(".{}", self.host))
    }
}

/// Makes `s` safe as a single path component.
fn file_name(s: &str) -> String {
    let s = s.replace('/', "_");
    if s.starts_with('.') { s.replacen('.', "_", 1) } else { s }
}

thread_local! {
    /// Logins filled in this session, so submitting one doesn't ask to save
    /// it again. Lives only in memory.
    static FILLED: RefCell<Vec<(Login, String)>> = const { RefCell::new(Vec::new()) };
}

pub fn remember_filled(login: &Login, password: &str) {
    FILLED.with(|f| {
        let mut filled = f.borrow_mut();
        filled.retain(|(l, _)| l != login);
        filled.push((login.clone(), password.to_string()));
    });
}

/// Whether `password` is what was last filled in for `username` on `host`.
pub fn was_filled(host: &str, username: &str, password: &str) -> bool {
    FILLED.with(|f| f.borrow().iter().any(|(l, p)| l.matches(host) && l.username == username && p == password))
}

/// Saved logins for a page at `host`, most specific domain first.
pub fn logins(host: &str) -> Vec<Login> {
    let mut domain = host;
    let mut found = Vec::new();
    // An IP address has no parent domains.
    let is_ip = host.split(':').next().is_some_and(|h| h.chars().all(|c| c.is_ascii_digit() || c == '.'));
    loop {
        found.extend(logins_in(domain));
        match domain.split_once('.').filter(|_| !is_ip) {
            // Stop before bare top-level domains.
            Some((_, parent)) if parent.contains('.') => domain = parent,
            _ => return found,
        }
    }
}

fn logins_in(host: &str) -> Vec<Login> {
    let Ok(entries) = std::fs::read_dir(paths::passwords_dir().join(file_name(host))) else { return Vec::new() };
    let mut logins: Vec<Login> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let username = name.strip_suffix(".gpg")?;
            Some(Login::new(host, if username == NO_USERNAME { "" } else { username }))
        })
        .collect();
    logins.sort_by(|a, b| a.username.cmp(&b.username));
    logins
}

/// Every saved login, by host.
pub fn all() -> Vec<Login> {
    let Ok(entries) = std::fs::read_dir(paths::passwords_dir()) else { return Vec::new() };
    let mut hosts: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    hosts.sort();
    hosts.iter().flat_map(|h| logins_in(h)).collect()
}

/// The key ids passwords are encrypted to, from `.gpg-id`.
pub fn keys() -> Vec<String> {
    std::fs::read_to_string(paths::passwords_dir().join(".gpg-id"))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// Secret keys in the user's keyring, as `(fingerprint, "Name <email>")`.
pub fn secret_keys() -> Vec<(String, String)> {
    let Ok(output) = gpg(&["--list-secret-keys", "--with-colons"], None) else { return Vec::new() };
    let mut keys: Vec<(String, String)> = Vec::new();
    let mut want_fpr = false;
    for line in String::from_utf8_lossy(&output).lines() {
        let fields: Vec<&str> = line.split(':').collect();
        match fields.first().copied() {
            Some("sec") => want_fpr = true,
            // The primary key's fingerprint follows its `sec` line; then
            // the first user id.
            Some("fpr") if want_fpr => {
                want_fpr = false;
                keys.push((fields.get(9).unwrap_or(&"").to_string(), String::new()));
            }
            Some("uid") => {
                if let Some((_, uid)) = keys.last_mut().filter(|(_, uid)| uid.is_empty()) {
                    *uid = unescape_colons(fields.get(9).unwrap_or(&""));
                }
            }
            _ => {}
        }
    }
    keys
}

/// gpg's `--with-colons` output escapes some characters as `\xNN`.
fn unescape_colons(s: &str) -> String {
    let mut out = Vec::new();
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'\\' {
            let hex: String = bytes.by_ref().skip(1).take(2).map(char::from).collect();
            out.extend(u8::from_str_radix(&hex, 16).ok());
        } else {
            out.push(b);
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Runs gpg, feeding it `input`; stdout on success, the reason on failure.
fn gpg(args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let mut child = Command::new("gpg")
        .args(["--batch", "--quiet"])
        .args(args)
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't run gpg: {e}"))?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(input).map_err(|e| e.to_string())?;
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let reason = stderr.lines().map(|l| l.trim_start_matches("gpg: ")).next_back().unwrap_or("gpg failed");
    Err(reason.to_string())
}

fn encrypt_to(path: &Path, contents: &str, keys: &[String]) -> Result<(), String> {
    if keys.is_empty() {
        return Err("choose a GPG key in Settings first".into());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("gpg.tmp");
    let tmp_str = tmp.to_string_lossy();
    let mut args = vec!["--yes", "--trust-model", "always", "--encrypt", "--output", &tmp_str];
    for key in keys {
        args.extend(["--recipient", key]);
    }
    gpg(&args, Some(contents.as_bytes()))?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn decrypt_file(path: &Path) -> Result<String, String> {
    let plain = gpg(&["--decrypt", &path.to_string_lossy()], None)?;
    String::from_utf8(plain).map_err(|_| "not text".to_string())
}

/// Saves (or replaces) a login. Encrypting needs no passphrase.
pub fn save(login: &Login, password: &str, url: &str) -> Result<(), String> {
    let contents = format!("{password}\nlogin: {}\nurl: {url}\n", login.username);
    encrypt_to(&login.path(), &contents, &keys())
}

/// The password for `login`; gpg-agent asks for the passphrase if needed.
pub fn decrypt(login: &Login) -> Result<String, String> {
    let contents = decrypt_file(&login.path())?;
    Ok(contents.lines().next().unwrap_or_default().to_string())
}

pub fn delete(login: &Login) {
    let path = login.path();
    let _ = std::fs::remove_file(&path);
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir(dir); // only if now empty
    }
}

/// Switches to encrypting with `key`, re-encrypting every saved login.
pub fn set_key(key: &str) -> Result<(), String> {
    let new_keys = vec![key.to_string()];
    let files: Vec<PathBuf> = all().iter().map(Login::path).collect();
    // Decrypt everything first, so a wrong passphrase changes nothing.
    let mut plain = Vec::new();
    for path in &files {
        plain.push(decrypt_file(path)?);
    }
    for (path, contents) in files.iter().zip(&plain) {
        encrypt_to(path, contents, &new_keys)?;
    }
    std::fs::create_dir_all(paths::passwords_dir()).map_err(|e| e.to_string())?;
    std::fs::write(paths::passwords_dir().join(".gpg-id"), format!("{key}\n")).map_err(|e| e.to_string())
}
