use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use reqwest::blocking::{Client, Response};
use reqwest::header::AUTHORIZATION;

use serde::Deserialize;
use serde_json::{self, Value, json};

const CACHE_NAME: &str = "rustyx";
const CONFIG_LOCATION: &str = "config.json";

#[derive(Deserialize)]
struct Config {
    client_id: String,
    client_secret: String,
}

fn parse_response(response: &mut Response) -> Result<Value, String> {
    let mut buf = "".to_string();
    if let Err(error) = response.read_to_string(&mut buf) {
        return Err(error.to_string());
    };

    return match serde_json::from_str(&buf) {
        Ok(parsed) => parsed,
        Err(error) => return Err(format!("Could not parse json: {error}")),
    };
}

fn extract_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn cache_file() -> Result<PathBuf, String> {
    let home = match env::var("HOME") {
        Ok(home) => home,
        Err(error) => return Err(error.to_string()),
    };

    let path = PathBuf::from(home).join(".cache").join(CACHE_NAME);

    match fs::create_dir_all(path.clone()) {
        Ok(_) => Ok(path.join(CACHE_NAME)),
        Err(error) => Err(error.to_string()),
    }
}

fn load_refresh_token() -> Option<String> {
    match cache_file() {
        Ok(path) => fs::read_to_string(path).ok(),
        Err(_) => None,
    }
}

fn save_refresh_token(refresh_token: String) -> Result<(), String> {
    match fs::write(cache_file()?, refresh_token) {
        Ok(_) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn prompt(msg: &str) -> String {
    print!("{}: ", msg);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_owned()
}

fn tokens_from_params(params: &HashMap<&str, String>) -> Result<(String, Option<String>), String> {
    let mut response = match Client::new()
        .post("https://api.dropbox.com/oauth2/token")
        .form(&params)
        .send()
        .and_then(|x| x.error_for_status())
    {
        Ok(response) => response,
        Err(err) => return Err(format!("Could not get the response: {err}")),
    };

    let parsed = parse_response(&mut response)?;

    match (
        parsed.get("access_token").and_then(extract_value),
        parsed.get("refresh_token").and_then(extract_value),
    ) {
        (Some(access_token), refresh_token) => Ok((access_token.to_string(), refresh_token)),
        _ => Err("Could not get tokens from the request".to_string()),
    }
}

fn authorize_by_code(
    client_id: &str,
    client_secret: &str,
) -> Result<(String, Option<String>), String> {
    let authorization_url = format!(
        "https://www.dropbox.com/oauth2/authorize?\
        client_id={client_id}&\
        token_access_type=offline\
        &response_type=code"
    );

    println!("{authorization_url}");
    let auth_code = prompt("Authorization code");
    let mut params = HashMap::new();
    params.insert("code", auth_code);
    params.insert("client_id", client_id.to_string());
    params.insert("client_secret", client_secret.to_string());
    params.insert("grant_type", "authorization_code".to_string());
    tokens_from_params(&params)
}

fn authorize_by_refresh_token(
    refresh_token: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<(String, Option<String>), String> {
    println!("Using the refresh token to authenticate...");
    let mut params = HashMap::new();
    params.insert("refresh_token", refresh_token.to_string());
    params.insert("grant_type", "refresh_token".to_string());
    params.insert("client_id", client_id.to_string());
    params.insert("client_secret", client_secret.to_string());
    tokens_from_params(&params)
}

struct RemoteFile {
    path: String,
    content_hash: String
}

impl RemoteFile {
    fn from_remote_folder(access_token: &str, folder: &str) -> Result<Vec<Self>, String> {
        let response = match Client::new()
            .post("https://api.dropboxapi.com/2/files/list_folder")
            .header(AUTHORIZATION, format!("Bearer {access_token}"))
            .json(&json!({
                "recursive": true,
                "path": "/onyx/Go103/Notepads",
            }))
            .send()
            .and_then(|x| x.error_for_status())
        {
            Ok(response) => response,
            Err(error) => {
                return Err(format!("Could not get the response during listing: {error}"))
            }
        };

        let parsed = parse_response(&mut response)?;

        parsed.get("access_token").and_then(extract_value),
        parsed.get("refresh_token").and_then(extract_value),
        ) {
            (Some(access_token), refresh_token) => Ok((access_token.to_string(), refresh_token)),
            _ => Err("Could not get tokens from the request".to_string()),
        }
}

fn main() {
    let config = match fs::read_to_string(CONFIG_LOCATION)
        .map_err(|_| ())
        .and_then(|x| serde_json::from_str::<Config>(&x).map_err(|_| ()))
    {
        Ok(config) => config,
        Err(_) => {
            eprintln!("Could not parse the configuration file");
            return;
        }
    };

    let (client_id, client_secret) = (config.client_id, config.client_secret);

    let result = match load_refresh_token() {
        Some(refresh_token) => {
            authorize_by_refresh_token(&refresh_token, &client_id, &client_secret)
        }
        None => authorize_by_code(&client_id, &client_secret),
    };

    let (access_token, refresh_token) = match result {
        Ok(tokens) => tokens,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };

    if let Some(refresh_token) = refresh_token {
        if let Err(error) = save_refresh_token(refresh_token) {
            eprintln!("{error}");
            return;
        };
    }

    println!("response {response:?}");
    println!("body {:?}", response.text_with_charset("utf-8").unwrap());
}
