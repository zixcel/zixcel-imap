use std::fs;
use std::process::ExitCode;

use serde::Serialize;
use serde_json::json;
use zixcel_imap::{build_plan, capabilities, doctor, parse_config, validation_report};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let error = json!({
                "schema": "zixcel://cli-error/v1",
                "connector": zixcel_imap::CONNECTOR,
                "status": "error",
                "message": message
            });
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&error)
                    .unwrap_or_else(|_| "{\"status\":\"error\"}".to_owned())
            );
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().ok_or_else(usage)?;
    match command.as_str() {
        "doctor" => {
            expect_no_more(&mut arguments)?;
            print_json(&doctor())
        }
        "capabilities" => {
            expect_no_more(&mut arguments)?;
            print_json(&capabilities())
        }
        "validate" => {
            let path = one_path(&mut arguments)?;
            let source = read_config(&path)?;
            let config = parse_config(&source).map_err(|error| error.to_string())?;
            print_json(&validation_report(&config).map_err(|error| error.to_string())?)
        }
        "plan" => {
            let path = one_path(&mut arguments)?;
            let source = read_config(&path)?;
            let config = parse_config(&source).map_err(|error| error.to_string())?;
            print_json(&build_plan(&config).map_err(|error| error.to_string())?)
        }
        _ => Err(usage()),
    }
}

fn one_path(arguments: &mut impl Iterator<Item = String>) -> Result<String, String> {
    let path = arguments.next().ok_or_else(usage)?;
    expect_no_more(arguments)?;
    Ok(path)
}

fn expect_no_more(arguments: &mut impl Iterator<Item = String>) -> Result<(), String> {
    if arguments.next().is_some() {
        Err(usage())
    } else {
        Ok(())
    }
}

fn read_config(path: &str) -> Result<String, String> {
    fs::read_to_string(path)
        .map_err(|_| "configuration file could not be read as UTF-8 text".to_owned())
}

fn print_json(value: &impl Serialize) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|_| "JSON serialization failed".to_owned())?
    );
    Ok(())
}

fn usage() -> String {
    "usage: zixcel-imap <doctor|capabilities|validate <config>|plan <config>>".to_owned()
}
