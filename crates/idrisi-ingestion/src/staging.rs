//! Per-attempt PostGIS staging namespace and reviewed flex-style rendering.
//!
//! Identifiers are derived only from the job UUID. The renderer accepts the
//! repository-owned style template and replaces its single schema declaration;
//! it never evaluates or interpolates job-provided SQL or Lua.

use std::{fs, io, path::Path};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagingNamespace {
    pub data_schema: String,
    pub middle_schema: String,
}

impl StagingNamespace {
    pub fn for_job(job_id: Uuid) -> Self {
        let compact = job_id.simple().to_string();
        Self {
            data_schema: format!("idrisi_stage_{compact}"),
            middle_schema: format!("idrisi_mid_{compact}"),
        }
    }

    /// Render the committed Bandung flex profile into an attempt-local file.
    /// Exactly four table declarations must be redirected; drift fails closed.
    pub fn render_flex_style(&self, template: &Path, destination: &Path) -> io::Result<()> {
        validate_identifier(&self.data_schema)?;
        let source = fs::read_to_string(template)?;
        let needle = "schema = 'tiles'";
        let count = source.matches(needle).count();
        if count != 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected 4 reviewed tiles schema declarations, found {count}"),
            ));
        }
        let rendered = source.replace(needle, &format!("schema = '{}'", self.data_schema));
        fs::write(destination, rendered)
    }
}

fn validate_identifier(value: &str) -> io::Result<()> {
    let mut chars = value.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        && value.len() <= 63;
    if valid {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsafe staging schema identifier",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("idrisi-{name}-{nonce}"))
    }

    #[test]
    fn derives_unique_bounded_identifiers_from_job_uuid() {
        let first = StagingNamespace::for_job(Uuid::nil());
        let second = StagingNamespace::for_job(Uuid::from_u128(1));
        assert_ne!(first.data_schema, second.data_schema);
        assert!(first.data_schema.starts_with("idrisi_stage_"));
        assert!(first.middle_schema.starts_with("idrisi_mid_"));
        assert!(first.data_schema.len() <= 63);
    }

    #[test]
    fn renders_only_reviewed_table_schema_declarations() {
        let template = temp_path("style-template");
        let output = temp_path("style-rendered");
        let source = "local a = { schema = 'tiles' }\nlocal b = { schema = 'tiles' }\nlocal c = { schema = 'tiles' }\nlocal d = { schema = 'tiles' }\n";
        fs::write(&template, source).unwrap();
        let namespace = StagingNamespace::for_job(Uuid::nil());
        namespace.render_flex_style(&template, &output).unwrap();
        let rendered = fs::read_to_string(&output).unwrap();
        assert_eq!(rendered.matches(&namespace.data_schema).count(), 4);
        assert!(!rendered.contains("schema = 'tiles'"));
        let _ = fs::remove_file(template);
        let _ = fs::remove_file(output);
    }

    #[test]
    fn rejects_unreviewed_style_template_drift() {
        let template = temp_path("style-drift");
        let output = temp_path("style-drift-output");
        fs::write(&template, "local a = { schema = 'tiles' }").unwrap();
        let namespace = StagingNamespace::for_job(Uuid::nil());
        assert_eq!(
            namespace
                .render_flex_style(&template, &output)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        let _ = fs::remove_file(template);
        let _ = fs::remove_file(output);
    }
}
