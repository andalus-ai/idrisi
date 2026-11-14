//! Provider-neutral ingestion primitives and the initial OSM importer adapter.
//!
//! This crate deliberately builds an argument vector rather than a shell command.
//! Credentials are supplied through the child process environment by the worker,
//! never through command-line arguments or persisted job metadata.

pub mod repository;
pub mod staging;
pub mod inspector;
pub mod worker;

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
            OsString::from(self.database.as_str()),
            OsString::from("--host"),
            OsString::from(self.host.as_str()),
            OsString::from("--port"),
            OsString::from(self.port.to_string()),
            OsString::from("--username"),
            OsString::from(self.username.as_str()),
            OsString::from("--schema"),
            OsString::from(self.schema.as_str()),
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
    let valid_rest = chars.all(|character| character == '_' || character.is_ascii_alphanumeric());

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

/// Bounded, HTTPS-only source acquisition. Redirects are disabled so a trusted
/// source host cannot redirect the worker to an untrusted/internal endpoint.
pub mod acquisition {
    use sha2::{Digest, Sha256};
    use std::{
        collections::HashSet,
        path::{Path, PathBuf},
        time::Duration,
    };
    use tokio::{
        fs::{self, OpenOptions},
        io::AsyncWriteExt,
        time::timeout,
    };

    #[derive(Debug, Clone)]
    pub struct DownloadPolicy {
        pub allowed_hosts: HashSet<String>,
        pub max_bytes: u64,
        pub request_timeout: Duration,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DownloadReceipt {
        pub path: PathBuf,
        pub bytes: u64,
        pub sha256: String,
    }

    #[derive(Debug)]
    pub enum DownloadError {
        InvalidPolicy,
        InvalidUrl,
        HostNotAllowed,
        Http(reqwest::Error),
        HttpStatus(u16),
        TooLarge,
        EmptyFile,
        Io(std::io::Error),
        TimedOut,
    }

    impl std::fmt::Display for DownloadError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::InvalidPolicy => write!(
                    f,
                    "download policy must allow hosts and a positive byte limit"
                ),
                Self::InvalidUrl => write!(f, "source URL must be a valid HTTPS URL"),
                Self::HostNotAllowed => write!(f, "source host is not in the configured allowlist"),
                Self::Http(error) => write!(f, "HTTP request failed: {error}"),
                Self::HttpStatus(status) => write!(f, "source returned HTTP status {status}"),
                Self::TooLarge => write!(f, "source exceeded the configured byte limit"),
                Self::EmptyFile => write!(f, "source file was empty"),
                Self::Io(error) => write!(f, "source file operation failed: {error}"),
                Self::TimedOut => write!(f, "source download timed out"),
            }
        }
    }
    impl std::error::Error for DownloadError {}

    pub async fn download(
        source_url: &str,
        destination: &Path,
        policy: &DownloadPolicy,
    ) -> Result<DownloadReceipt, DownloadError> {
        if policy.allowed_hosts.is_empty() || policy.max_bytes == 0 {
            return Err(DownloadError::InvalidPolicy);
        }
        let url = reqwest::Url::parse(source_url).map_err(|_| DownloadError::InvalidUrl)?;
        if url.scheme() != "https" || url.username() != "" || url.password().is_some() {
            return Err(DownloadError::InvalidUrl);
        }
        let host = url.host_str().ok_or(DownloadError::InvalidUrl)?;
        if !policy
            .allowed_hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
        {
            return Err(DownloadError::HostNotAllowed);
        }

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(policy.request_timeout)
            .build()
            .map_err(DownloadError::Http)?;

        let operation = async {
            let mut response = client.get(url).send().await.map_err(DownloadError::Http)?;
            if !response.status().is_success() {
                return Err(DownloadError::HttpStatus(response.status().as_u16()));
            }
            if response
                .content_length()
                .is_some_and(|length| length > policy.max_bytes)
            {
                return Err(DownloadError::TooLarge);
            }

            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)
                .await
                .map_err(DownloadError::Io)?;
            let mut hasher = Sha256::new();
            let mut total = 0_u64;

            while let Some(chunk) = response.chunk().await.map_err(DownloadError::Http)? {
                total = total
                    .checked_add(chunk.len() as u64)
                    .ok_or(DownloadError::TooLarge)?;
                if total > policy.max_bytes {
                    drop(file);
                    let _ = fs::remove_file(destination).await;
                    return Err(DownloadError::TooLarge);
                }
                hasher.update(&chunk);
                file.write_all(&chunk).await.map_err(DownloadError::Io)?;
            }

            if total == 0 {
                drop(file);
                let _ = fs::remove_file(destination).await;
                return Err(DownloadError::EmptyFile);
            }
            file.sync_all().await.map_err(DownloadError::Io)?;
            Ok(DownloadReceipt {
                path: destination.to_path_buf(),
                bytes: total,
                sha256: format!("{:x}", hasher.finalize()),
            })
        };

        match timeout(policy.request_timeout, operation).await {
            Ok(Ok(receipt)) => Ok(receipt),
            Ok(Err(error)) => {
                // Remove partial files on every failure; never reuse an incomplete PBF.
                let _ = fs::remove_file(destination).await;
                Err(error)
            }
            Err(_) => {
                let _ = fs::remove_file(destination).await;
                Err(DownloadError::TimedOut)
            }
        }
    }
}

/// Minimal process supervisor for the pinned osm2pgsql adapter. Output is
/// discarded by default to prevent unbounded pipe buffering or secret leakage.
/// Operators should capture redacted logs at the container/runtime boundary.
pub mod process {
    use crate::Osm2pgsqlConfig;
    use std::{path::PathBuf, process::Stdio, time::Duration};
    use tokio::{process::Command, time::timeout};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ImportResult {
        pub exit_code: Option<i32>,
    }

    #[derive(Debug)]
    pub enum ImportError {
        InvalidConfig(crate::ConfigError),
        Spawn(std::io::Error),
        TimedOut,
        Failed(Option<i32>),
    }

    impl std::fmt::Display for ImportError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::InvalidConfig(error) => write!(f, "invalid importer configuration: {error}"),
                Self::Spawn(error) => write!(f, "could not start importer: {error}"),
                Self::TimedOut => write!(f, "osm2pgsql exceeded its execution deadline"),
                Self::Failed(code) => write!(f, "osm2pgsql exited unsuccessfully: {code:?}"),
            }
        }
    }
    impl std::error::Error for ImportError {}

    pub async fn run(
        config: &Osm2pgsqlConfig,
        database_password: &str,
        deadline: Duration,
    ) -> Result<ImportResult, ImportError> {
        let invocation = config.invocation().map_err(ImportError::InvalidConfig)?;
        let mut child = Command::new(invocation.program)
            .args(invocation.args)
            .env("PGPASSWORD", database_password)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(ImportError::Spawn)?;

        match timeout(deadline, child.wait()).await {
            Ok(Ok(status)) if status.success() => Ok(ImportResult {
                exit_code: status.code(),
            }),
            Ok(Ok(status)) => Err(ImportError::Failed(status.code())),
            Ok(Err(error)) => Err(ImportError::Spawn(error)),
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                Err(ImportError::TimedOut)
            }
        }
    }

    /// Stable helper for callers that derive per-job storage paths.
    pub fn job_input_path(root: &std::path::Path, job_id: &str) -> PathBuf {
        root.join(job_id).join("source.osm.pbf")
    }
}

#[cfg(test)]
mod acquisition_tests {
    use super::acquisition::{download, DownloadError, DownloadPolicy};
    use std::{collections::HashSet, path::PathBuf, time::Duration};

    fn policy() -> DownloadPolicy {
        DownloadPolicy {
            allowed_hosts: HashSet::from(["download.example.org".to_owned()]),
            max_bytes: 1024,
            request_timeout: Duration::from_secs(2),
        }
    }

    #[tokio::test]
    async fn rejects_non_https_sources_before_network_access() {
        let result = download(
            "http://download.example.org/indonesia.osm.pbf",
            &PathBuf::from("/tmp/idrisi-test-never-created.pbf"),
            &policy(),
        )
        .await;
        assert!(matches!(result, Err(DownloadError::InvalidUrl)));
    }

    #[tokio::test]
    async fn rejects_hosts_outside_operator_allowlist() {
        let result = download(
            "https://attacker.example/indonesia.osm.pbf",
            &PathBuf::from("/tmp/idrisi-test-never-created.pbf"),
            &policy(),
        )
        .await;
        assert!(matches!(result, Err(DownloadError::HostNotAllowed)));
    }

    #[tokio::test]
    async fn rejects_empty_allowlist() {
        let mut policy = policy();
        policy.allowed_hosts.clear();
        let result = download(
            "https://download.example.org/indonesia.osm.pbf",
            &PathBuf::from("/tmp/idrisi-test-never-created.pbf"),
            &policy,
        )
        .await;
        assert!(matches!(result, Err(DownloadError::InvalidPolicy)));
    }
}

