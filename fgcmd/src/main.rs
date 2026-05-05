const COMMAND_DATA: &str = include_str!("../commands.csv");
const DEFAULT_BASE_URL: &str = "http://localhost:8080";
const BASE_URL_ENV: &str = "FGCMD_BASE_URL";

use std::env;
use std::collections::HashMap;

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

fn command_arg() -> Result<String, String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        return Err("usage: fgcmd <COMMAND>".to_string());
    };

    if args.next().is_some() {
        return Err("usage: fgcmd <COMMAND>".to_string());
    }

    Ok(command)
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

fn execute_command(spec: &CommandSpec) -> Result<(), String> {
    let base_url = env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    let target_url = build_request_url(&base_url, &spec.url)?;

    let client = reqwest::blocking::Client::new();
    let body = spec.body.trim();

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

fn main() {
    let lookup = build_command_lookup(COMMAND_DATA).expect("failed to parse commands.csv");

    let command = match command_arg() {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let spec = match lookup.get(&command) {
        Some(spec) => spec,
        None => {
            eprintln!("unknown command: {command}");
            std::process::exit(1);
        }
    };

    if let Err(message) = execute_command(spec) {
        eprintln!("{message}");
        std::process::exit(1);
    }
}
