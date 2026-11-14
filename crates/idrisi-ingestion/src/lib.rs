//! Provider-neutral ingestion primitives and the initial OSM importer adapter.
//!
//! This crate deliberately builds an argument vector rather than a shell command.
//! Credentials are supplied through the child process environment by the worker,
//! never through command-line arguments or persisted job metadata.

use std::{ffi::OsString, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Osm2pgsqlConfig {
    pub executable: PathBuf,
    pub pbf_path: PathBuf,
    pub style_path: PathBuf,
    pub database: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub schema: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImporterInvocation {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    EmptyField(&'static str),
    InvalidIdentifier(&'static str),
    InvalidPath(&'static str),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::InvalidIdentifier(field) => {
                write!(f, "{field} must be a safe SQL identifier")
            }
            Self::InvalidPath(field) => write!(f, "{field} must be a non-empty path"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Osm2pgsqlConfig {
    /// Build a deterministic argv for osm2pgsql's flex output.
    ///
    /// The style file must be a reviewed, version-controlled Idrisi recipe.
    /// The caller must set PGPASSWORD (or configure a protected pgpass file)
    /// on the spawned process; this method intentionally has no password field.
    pub fn invocation(&self) -> Result<ImporterInvocation, ConfigError> {
        validate_path(&self.executable, "executable")?;
        validate_path(&self.pbf_path, "pbf_path")?;
        validate_path(&self.style_path, "style_path")?;
        validate_identifier(&self.database, "database")?;
        validate_identifier(&self.username, "username")?;
        validate_identifier(&self.schema, "schema")?;
        if self.host.trim().is_empty() {
            return Err(ConfigError::EmptyField("host"));
        }
        if self.port == 0 {
            return Err(ConfigError::EmptyField("port"));
        }

        let args = vec![
            OsString::from("--create"),
            OsString::from("--slim"),
            OsString::from("--output=flex"),
            OsString::from("--style"),
            self.style_path.as_os_str().to_owned(),
            OsString::from("--database"),
            OsString::from(&self.database),
            OsString::from("--host"),
            OsString::from(&self.host),
            OsString::from("--port"),
            OsString::from(self.port.to_string()),
            OsString::from("--username"),
            OsString::from(&self.username),
            OsString::from("--schema"),
            OsString::from(&self.schema),
            self.pbf_path.as_os_str().to_owned(),
        ];

        Ok(ImporterInvocation {
            program: self.executable.clone(),
            args,
        })
    }
}

fn validate_path(path: &std::path::Path, field: &'static str) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() {
        return Err(ConfigError::InvalidPath(field));
    }
    Ok(())
}

fn validate_identifier(value: &str, field: &'static str) -> Result<(), ConfigError> {
    let mut chars = value.chars();
    let valid_first = chars
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic());
    let valid_rest = chars.all(|character| {
        character == '_' || character.is_ascii_alphanumeric()
    });

    if !valid_first || !valid_rest || value.len() > 63 {
        return Err(ConfigError::InvalidIdentifier(field));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Osm2pgsqlConfig {
        Osm2pgsqlConfig {
            executable: "/usr/bin/osm2pgsql".into(),
            pbf_path: "/data/staging/job-1/input.osm.pbf".into(),
            style_path: "/opt/idrisi/styles/indonesia.lua".into(),
            database: "idrisi".into(),
            host: "postgis".into(),
            port: 5432,
            username: "idrisi_import".into(),
            schema: "osm_stage_12345678".into(),
        }
    }

    #[test]
    fn builds_argument_vector_without_shell_interpolation() {
        let invocation = config().invocation().expect("valid config");
        assert_eq!(invocation.program, PathBuf::from("/usr/bin/osm2pgsql"));
        assert!(invocation.args.iter().any(|arg| arg == "--output=flex"));
        assert!(invocation
            .args
            .iter()
            .any(|arg| arg == "/data/staging/job-1/input.osm.pbf"));
        assert!(!invocation.args.iter().any(|arg| arg == ";"));
    }

    #[test]
    fn rejects_unsafe_schema_identifier() {
        let mut config = config();
        config.schema = "stage; DROP SCHEMA public".into();
        assert_eq!(
            config.invocation().unwrap_err(),
            ConfigError::InvalidIdentifier("schema")
        );
    }

    #[test]
    fn rejects_overlong_postgres_identifier() {
        let mut config = config();
        config.schema = "a".repeat(64);
        assert_eq!(
            config.invocation().unwrap_err(),
            ConfigError::InvalidIdentifier("schema")
        );
    }

    #[test]
    fn rejects_missing_style_path() {
        let mut config = config();
        config.style_path = PathBuf::new();
        assert_eq!(
            config.invocation().unwrap_err(),
            ConfigError::InvalidPath("style_path")
        );
    }
}
