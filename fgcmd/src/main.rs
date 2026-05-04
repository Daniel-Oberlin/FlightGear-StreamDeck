const COMMAND_DATA: &str = include_str!("../commands.csv");

use std::collections::HashMap;

use serde::Deserialize;

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

    let command = "DME_NAV1";
    if let Some(spec) = lookup.get(command) {
        println!("Command: {command}");
        println!("URL: {}", spec.url);
        println!("Body: {}", spec.body);
    }
}
