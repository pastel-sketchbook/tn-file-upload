use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing {0}")]
    Missing(String),
    #[error("invalid {0}: {1}")]
    Invalid(String, String),
}

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,
    pub auth_token: String,
    pub storage_path: String,
    pub max_file_size: u64,
    pub chunk_size: usize,
}

impl Config {
    /// # Errors
    ///
    /// Returns `ConfigError` if required env vars are missing or invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_env_fn(|k| std::env::var(k))
    }

    fn from_env_fn<F>(env: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Result<String, std::env::VarError>,
    {
        let max_file_size = env("MAX_FILE_SIZE")
            .unwrap_or_else(|_| "104857600".into()) // 100 MiB
            .parse::<u64>()
            .map_err(|e| ConfigError::Invalid("MAX_FILE_SIZE".into(), e.to_string()))?;

        let chunk_size = env("CHUNK_SIZE")
            .unwrap_or_else(|_| "65536".into()) // 64 KiB
            .parse::<usize>()
            .map_err(|e| ConfigError::Invalid("CHUNK_SIZE".into(), e.to_string()))?;

        Ok(Self {
            listen_addr: env("LISTEN_ADDR").unwrap_or_else(|_| "[::]:50051".into()),
            auth_token: env("AUTH_TOKEN").map_err(|_| ConfigError::Missing("AUTH_TOKEN".into()))?,
            storage_path: env("STORAGE_PATH").unwrap_or_else(|_| "./uploads".into()),
            max_file_size,
            chunk_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_auth_token_returns_error() {
        let result = Config::from_env_fn(|key| match key {
            "AUTH_TOKEN" => Err(std::env::VarError::NotPresent),
            _ => Err(std::env::VarError::NotPresent),
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("AUTH_TOKEN"));
    }

    #[test]
    fn valid_config_with_defaults() {
        let result = Config::from_env_fn(|key| match key {
            "AUTH_TOKEN" => Ok("test-token".into()),
            _ => Err(std::env::VarError::NotPresent),
        });
        let config = result.unwrap();
        assert_eq!(config.listen_addr, "[::]:50051");
        assert_eq!(config.chunk_size, 65536);
        assert_eq!(config.max_file_size, 104_857_600);
    }

    #[test]
    fn invalid_max_file_size_returns_error() {
        let result = Config::from_env_fn(|key| match key {
            "AUTH_TOKEN" => Ok("tok".into()),
            "MAX_FILE_SIZE" => Ok("not_a_number".into()),
            _ => Err(std::env::VarError::NotPresent),
        });
        assert!(result.is_err());
    }
}
