use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub api_token: String,
    #[serde(default)]
    pub api_token_file: Option<PathBuf>,
    #[serde(default)]
    pub admin_credentials_file: Option<PathBuf>,
}

impl AuthConfig {
    /// Token from the inline field, or the contents of `api_token_file`.
    pub fn resolved_token(&self) -> Result<String, curatarr_core::error::ConfigError> {
        if !self.api_token.is_empty() {
            return Ok(self.api_token.clone()); // clone: caller owns the secret string
        }
        let Some(path) = &self.api_token_file else {
            return Ok(String::new());
        };
        let raw = std::fs::read_to_string(path).map_err(|e| {
            curatarr_core::error::ConfigError::LoadFailed(format!(
                "auth token file {}: {e}",
                path.display()
            ))
        })?;
        Ok(raw.trim().to_string())
    }

    /// `username = "..."\npassword = "..."` from a credential file, not Nix-store TOML.
    pub fn load_admin_credentials(
        &self,
    ) -> Result<Option<(String, String)>, curatarr_core::error::ConfigError> {
        let Some(path) = &self.admin_credentials_file else {
            return Ok(None);
        };
        let raw = std::fs::read_to_string(path).map_err(|e| {
            curatarr_core::error::ConfigError::LoadFailed(format!(
                "admin credentials file {}: {e}",
                path.display()
            ))
        })?;
        parse_admin_credentials(&raw).map(Some)
    }
}

#[derive(Debug, Deserialize)]
struct AdminFile {
    username: String,
    password: String,
}

pub fn parse_admin_credentials(
    raw: &str,
) -> Result<(String, String), curatarr_core::error::ConfigError> {
    let parsed: AdminFile = toml::from_str(raw).map_err(|e| {
        curatarr_core::error::ConfigError::LoadFailed(format!("admin credentials: {e}"))
    })?;
    if parsed.username.is_empty() || parsed.password.is_empty() {
        return Err(curatarr_core::error::ConfigError::InvalidValue {
            field: "admin_credentials".into(),
            reason: "username and password must not be empty".into(),
        });
    }
    Ok((parsed.username, parsed.password))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_token_wins() {
        let cfg = AuthConfig {
            api_token: "abc".into(),
            api_token_file: None,
            admin_credentials_file: None,
        };
        assert_eq!(cfg.resolved_token().unwrap(), "abc");
    }

    #[test]
    fn parses_admin_file() {
        let (user, pass) =
            parse_admin_credentials("username = \"admin\"\npassword = \"s3cret\"\n").unwrap();
        assert_eq!(user, "admin");
        assert_eq!(pass, "s3cret");
    }
}
