const COMMAND_DATA: &str = include_str!("../commands.csv");
const DEFAULT_BASE_URL: &str = "http://localhost:8080";
const BASE_URL_ENV: &str = "FGCMD_BASE_URL";
const DEFAULT_LOCK_DIR_ENV: &str = "FGCMD_LOCK_DIR";
const DEFAULT_INITIAL_DELAY_MS: u64 = 500;
const INITIAL_DELAY_MS_ENV: &str = "FGCMD_INITIAL_DELAY_MS";
const DEFAULT_REPEAT_DELAY_MS: u64 = 30;
const REPEAT_DELAY_MS_ENV: &str = "FGCMD_REPEAT_DELAY_MS";
const DEFAULT_MAX_DURATION_MS: u64 = 5000;
const MAX_DURATION_MS_ENV: &str = "FGCMD_MAX_DURATION_MS";

use std::env;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use reqwest::Url;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct CommandRow {
    #[serde(rename = "Command")]
    command: String,
    #[serde(rename = "URL")]
    url: String,
    #[serde(rename = "Body")]
    body: String,
}

#[derive(Debug)]
struct CommandSpec {
    url: String,
    body: String,
}

struct CliArgs {
    command: String,
    pretend: bool,
}

fn command_arg() -> Result<CliArgs, String> {
    let mut args = env::args().skip(1);
    let Some(first_arg) = args.next() else {
        return Err("usage: fgcmd [--pretend] <COMMAND>".to_string());
    };

    let (pretend, command) = if first_arg == "--pretend" {
        let Some(command) = args.next() else {
            return Err("usage: fgcmd [--pretend] <COMMAND>".to_string());
        };
        (true, command)
    } else {
        (false, first_arg)
    };

    if args.next().is_some() {
        return Err("usage: fgcmd [--pretend] <COMMAND>".to_string());
    }

    Ok(CliArgs { command, pretend })
}

fn path_and_query(url: &str) -> Result<String, String> {
    let parsed = Url::parse(url).map_err(|err| format!("invalid command URL '{url}': {err}"))?;
    let mut out = parsed.path().to_string();
    if let Some(query) = parsed.query() {
        out.push('?');
        out.push_str(query);
    }
    Ok(out)
}

fn build_request_url(base_url: &str, command_url: &str) -> Result<String, String> {
    let base = Url::parse(base_url)
        .map_err(|err| format!("invalid base URL in {BASE_URL_ENV}: {err}"))?;
    let suffix = path_and_query(command_url)?;

    base.join(&suffix)
        .map(|url| url.to_string())
        .map_err(|err| format!("failed to build target URL from base '{base_url}' and suffix '{suffix}': {err}"))
}

fn execute_command(spec: &CommandSpec, pretend: bool) -> Result<(), String> {
    let base_url = env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    let target_url = build_request_url(&base_url, &spec.url)?;
    let body = spec.body.trim();

    if pretend {
        if body.is_empty() {
            println!("POST {target_url} body=<empty>");
        } else {
            println!("POST {target_url} body={body}");
        }
        return Ok(());
    }

    let client = reqwest::blocking::Client::new();

    let response = if body.is_empty() {
        client
            .post(&target_url)
            .send()
            .map_err(|err| format!("POST {target_url} failed: {err}"))?
    } else {
        let json_body: Value = serde_json::from_str(body)
            .map_err(|err| format!("invalid JSON body in commands.csv: {err}"))?;

        client
            .post(&target_url)
            .json(&json_body)
            .send()
            .map_err(|err| format!("POST {target_url} failed: {err}"))?
    };

    if !response.status().is_success() {
        return Err(format!(
            "POST {target_url} returned HTTP {}",
            response.status()
        ));
    }

    Ok(())
}

fn build_command_lookup(data: &str) -> Result<HashMap<String, CommandSpec>, csv::Error> {
    let mut reader = csv::Reader::from_reader(data.as_bytes());

    reader
        .deserialize::<CommandRow>()
        .map(|result| {
            result.map(|row| {
                (
                    row.command,
                    CommandSpec {
                        url: row.url,
                        body: row.body,
                    },
                )
            })
        })
        .collect()
}

fn get_lock_dir() -> Result<PathBuf, String> {
    if let Ok(dir) = env::var(DEFAULT_LOCK_DIR_ENV) {
        Ok(PathBuf::from(dir))
    } else {
        env::temp_dir()
            .canonicalize()
            .map_err(|err| format!("failed to get temp dir: {err}"))
    }
}

fn get_lock_path(lock_dir: &Path, command: &str) -> PathBuf {
    lock_dir.join(format!("fgcmd_{}.lock", command))
}

fn read_env_duration(env_var: &str, default: u64) -> Duration {
    let millis = env::var(env_var)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(default);
    Duration::from_millis(millis)
}

fn handle_command_with_lock(
    command: &str,
    spec: &CommandSpec,
    pretend: bool,
) -> Result<(), String> {
    let lock_dir = get_lock_dir()?;
    let lock_path = get_lock_path(&lock_dir, command);

    if lock_path.exists() {
        // Button is being released; delete lock file and exit
        fs::remove_file(&lock_path)
            .map_err(|err| format!("failed to delete lock file: {err}"))?;
        return Ok(());
    }

    // Button is being pressed; create lock file and proceed
    fs::write(&lock_path, "")
        .map_err(|err| format!("failed to create lock file: {err}"))?;

    // Send initial command
    execute_command(spec, pretend)?;

    // Get timing constants from env or defaults
    let initial_delay = read_env_duration(INITIAL_DELAY_MS_ENV, DEFAULT_INITIAL_DELAY_MS);
    let repeat_delay = read_env_duration(REPEAT_DELAY_MS_ENV, DEFAULT_REPEAT_DELAY_MS);
    let max_duration = read_env_duration(MAX_DURATION_MS_ENV, DEFAULT_MAX_DURATION_MS);

    // Sleep before loop
    thread::sleep(initial_delay);

    let start = Instant::now();
    loop {
        // Check if total time exceeded
        if start.elapsed() >= max_duration {
            break;
        }

        // Check if lock file still exists
        if !lock_path.exists() {
            break;
        }

        // Send command again
        execute_command(spec, pretend)?;
        thread::sleep(repeat_delay);
    }

    // Clean up lock file
    let _ = fs::remove_file(&lock_path);
    Ok(())
}

fn main() {
    let lookup = build_command_lookup(COMMAND_DATA).expect("failed to parse commands.csv");

    let args = match command_arg() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let spec = match lookup.get(&args.command) {
        Some(spec) => spec,
        None => {
            eprintln!("unknown command: {}", args.command);
            std::process::exit(1);
        }
    };

    if let Err(message) = handle_command_with_lock(&args.command, spec, args.pretend) {
        eprintln!("{message}");
        std::process::exit(1);
    }
}
