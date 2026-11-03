use super::banner::BANNER;
use anyhow::{Error, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{
  fs,
  io::stdin,
  path::{Path, PathBuf},
};

const DEFAULT_PORT: u16 = 8888;
const FILE_NAME: &str = "client.yml";
const CONFIG_DIR: &str = ".config";
const APP_CONFIG_DIR: &str = "spotify-tui";
const TOKEN_CACHE_FILE: &str = ".spotify_token_cache.json";

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientConfig {
  pub client_id: String,
  pub device_id: Option<String>,
  // FIXME: port should be defined in `user_config` not in here
  pub port: Option<u16>,
}

pub struct ConfigPaths {
  pub config_file_path: PathBuf,
  pub token_cache_path: PathBuf,
}

fn parse_client_config(input: &str) -> Result<ClientConfig> {
  // Parser diagnostics can include neighboring source lines with an old secret.
  crate::yaml::from_str(input)
    .map_err(|_| anyhow!("Invalid client.yml; check the Client ID, device and port settings"))
}

impl ClientConfig {
  pub fn new() -> ClientConfig {
    ClientConfig {
      client_id: "".to_string(),
      device_id: None,
      port: None,
    }
  }

  pub fn get_redirect_uri(&self) -> String {
    format!("http://127.0.0.1:{}/callback", self.get_port())
  }

  pub fn get_port(&self) -> u16 {
    self.port.unwrap_or(DEFAULT_PORT)
  }

  pub fn get_or_build_paths(&self) -> Result<ConfigPaths> {
    match dirs::home_dir() {
      Some(home) => {
        let path = Path::new(&home);
        let home_config_dir = path.join(CONFIG_DIR);
        let app_config_dir = home_config_dir.join(APP_CONFIG_DIR);

        if !home_config_dir.exists() {
          fs::create_dir(&home_config_dir)?;
        }

        crate::auth::private_directory(&app_config_dir)?;

        let config_file_path = &app_config_dir.join(FILE_NAME);
        let token_cache_path = &app_config_dir.join(TOKEN_CACHE_FILE);

        let paths = ConfigPaths {
          config_file_path: config_file_path.to_path_buf(),
          token_cache_path: token_cache_path.to_path_buf(),
        };

        Ok(paths)
      }
      None => Err(anyhow!("No $HOME directory found for client config")),
    }
  }

  pub fn set_device_id(&mut self, device_id: String) -> Result<()> {
    let paths = self.get_or_build_paths()?;
    let config_string = String::from_utf8(crate::auth::read_private(
      &paths.config_file_path,
      crate::http::TOKEN_BODY_LIMIT,
    )?)
    .map_err(|_| anyhow!("Client settings are not UTF-8"))?;
    let mut config_yml: ClientConfig = parse_client_config(&config_string)?;

    self.device_id = Some(device_id.clone());
    config_yml.device_id = Some(device_id);

    let new_config = serde_yaml::to_string(&config_yml)?;
    crate::auth::write_private(&paths.config_file_path, new_config.as_bytes())?;
    Ok(())
  }

  pub fn load_config(&mut self) -> Result<()> {
    let paths = self.get_or_build_paths()?;
    if paths.config_file_path.exists() {
      let config_string = String::from_utf8(crate::auth::read_private(
        &paths.config_file_path,
        crate::http::TOKEN_BODY_LIMIT,
      )?)
      .map_err(|_| anyhow!("Client settings are not UTF-8"))?;
      let config_yml: ClientConfig = parse_client_config(&config_string)?;

      Self::validate_client_key(&config_yml.client_id)?;
      if config_yml.port == Some(0) {
        return Err(anyhow!("Redirect port must be between 1 and 65535"));
      }
      self.client_id = config_yml.client_id;
      self.device_id = config_yml.device_id;
      self.port = config_yml.port;
      // Drop a legacy client_secret from disk; PKCE needs only the public ID.
      if crate::yaml::from_str::<std::collections::BTreeMap<String, serde::de::IgnoredAny>>(
        &config_string,
      )
      .map_err(|_| anyhow!("Invalid client.yml"))?
      .contains_key("client_secret")
      {
        crate::auth::write_private(
          &paths.config_file_path,
          serde_yaml::to_string(self)?.as_bytes(),
        )?;
      }

      Ok(())
    } else {
      println!("{}", BANNER);

      println!(
        "Config will be saved to {}",
        paths.config_file_path.display()
      );

      println!("\nHow to get setup:\n");

      let instructions = [
        "Go to the Spotify dashboard - https://developer.spotify.com/dashboard",
        "Create an app (Spotify Premium is required)",
        "Open the app settings and enable Web API",
        &format!(
          "Add `http://127.0.0.1:{}/callback` to the Redirect URIs",
          DEFAULT_PORT
        ),
        "You are now ready to authenticate with Spotify!",
      ];

      for (number, item) in (1..).zip(instructions.iter()) {
        println!("  {}. {}", number, item);
      }

      let client_id = ClientConfig::get_client_key_from_input("Client ID")?;

      let mut port = String::new();
      println!("\nEnter port of redirect uri (default {}): ", DEFAULT_PORT);
      stdin().read_line(&mut port)?;
      let port = if port.trim().is_empty() {
        DEFAULT_PORT
      } else {
        port.trim().parse::<u16>()?
      };
      if port == 0 {
        return Err(anyhow!("Redirect port must be between 1 and 65535"));
      }

      let config_yml = ClientConfig {
        client_id,
        device_id: None,
        port: Some(port),
      };

      let content_yml = serde_yaml::to_string(&config_yml)?;

      crate::auth::write_private(&paths.config_file_path, content_yml.as_bytes())?;

      self.client_id = config_yml.client_id;
      self.device_id = config_yml.device_id;
      self.port = config_yml.port;

      Ok(())
    }
  }

  fn get_client_key_from_input(type_label: &'static str) -> Result<String> {
    let mut client_key = String::new();
    const MAX_RETRIES: u8 = 5;
    let mut num_retries = 0;
    loop {
      println!("\nEnter your {}: ", type_label);
      stdin().read_line(&mut client_key)?;
      client_key = client_key.trim().to_string();
      match ClientConfig::validate_client_key(&client_key) {
        Ok(_) => return Ok(client_key),
        Err(error_string) => {
          println!("{}", error_string);
          client_key.clear();
          num_retries += 1;
          if num_retries == MAX_RETRIES {
            return Err(Error::from(std::io::Error::other(format!(
              "Maximum retries ({}) exceeded.",
              MAX_RETRIES
            ))));
          }
        }
      };
    }
  }

  fn validate_client_key(key: &str) -> Result<()> {
    const EXPECTED_LEN: usize = 32;
    if key.len() != EXPECTED_LEN {
      Err(Error::from(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("invalid length: {} (must be {})", key.len(), EXPECTED_LEN,),
      )))
    } else if !key.chars().all(|c| c.is_ascii_hexdigit()) {
      Err(Error::from(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "invalid character found (must be hex digits)",
      )))
    } else {
      Ok(())
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn credential_parse_errors_do_not_echo_secret_source_lines() {
    let error =
      parse_client_config("client_id: [bad]\nclient_secret: never-print-this\n").unwrap_err();
    assert!(!format!("{error:?}").contains("never-print-this"));
  }

  #[test]
  fn legacy_settings_load_without_requiring_or_serializing_a_secret() {
    let config: ClientConfig = crate::yaml::from_str("client_id: 0123456789abcdef0123456789abcdef\nclient_secret: obsolete\ndevice_id: saved-device\nport: 9999\n").unwrap();
    assert_eq!(config.device_id.as_deref(), Some("saved-device"));
    assert_eq!(config.get_redirect_uri(), "http://127.0.0.1:9999/callback");
    assert!(
      !serde_yaml::to_string(&config)
        .unwrap()
        .contains("client_secret")
    );
    assert_eq!(
      ClientConfig::new().get_redirect_uri(),
      "http://127.0.0.1:8888/callback"
    );
  }
}
