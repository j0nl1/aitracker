use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

fn line_defines_key(line: &str, key: &str) -> bool {
    let assignment = line
        .trim_start()
        .strip_prefix("export ")
        .unwrap_or_else(|| line.trim_start());
    assignment
        .split_once('=')
        .map(|(candidate, _)| candidate.trim() == key)
        .unwrap_or(false)
}

pub fn update_env_contents(contents: &str, key: &str, value: Option<&str>) -> String {
    let replacement = value.map(|secret| {
        format!(
            "{}={}\n",
            key,
            serde_json::to_string(secret).expect("serializing a string cannot fail")
        )
    });
    let mut output =
        String::with_capacity(contents.len() + replacement.as_deref().map_or(0, str::len));
    let mut replaced = false;

    for line in contents.split_inclusive('\n') {
        if line_defines_key(line, key) {
            if !replaced {
                if let Some(replacement) = &replacement {
                    output.push_str(replacement);
                }
                replaced = true;
            }
        } else {
            output.push_str(line);
        }
    }

    if !replaced {
        if let Some(replacement) = replacement {
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(&replacement);
        }
    }

    output
}

pub fn write_env_file(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(contents.as_bytes())?;

    Ok(())
}

pub fn set_env_value(path: &Path, key: &str, value: Option<&str>) -> Result<()> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if value.is_none() {
                return Ok(());
            }
            String::new()
        }
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to read {}", path.display()))
        }
    };
    let updated = update_env_contents(&contents, key, value);
    write_env_file(path, &updated).with_context(|| format!("Failed to write {}", path.display()))
}

pub fn read_env_value(path: &Path, key: &str) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let entries = dotenvy::from_path_iter(path)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    for entry in entries {
        let (candidate, value) =
            entry.with_context(|| format!("Failed to parse {}", path.display()))?;
        if candidate == key {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn load_env_file(path: &Path) -> Result<()> {
    if path.exists() {
        dotenvy::from_path(path).with_context(|| format!("Failed to load {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_adds_quoted_value_without_disturbing_existing_entries() {
        let original = "# managed locally\nOTHER_KEY=untouched\n";
        let updated = update_env_contents(original, "OPENAI_API_KEY", Some("sk-a b\"c"));

        assert_eq!(
            updated,
            "# managed locally\nOTHER_KEY=untouched\nOPENAI_API_KEY=\"sk-a b\\\"c\"\n"
        );
    }

    #[test]
    fn update_replaces_an_existing_exported_value() {
        let original = "export GITHUB_TOKEN=old\nOTHER_KEY=value\n";
        let updated = update_env_contents(original, "GITHUB_TOKEN", Some("new"));

        assert_eq!(updated, "GITHUB_TOKEN=\"new\"\nOTHER_KEY=value\n");
    }

    #[test]
    fn update_removes_only_the_requested_value() {
        let original = "GITHUB_TOKEN=old\nGITHUB_TOKEN_SUFFIX=keep\n";
        let updated = update_env_contents(original, "GITHUB_TOKEN", None);

        assert_eq!(updated, "GITHUB_TOKEN_SUFFIX=keep\n");
    }

    #[cfg(unix)]
    #[test]
    fn write_env_file_uses_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let path = std::env::temp_dir().join(format!("ait-secrets-test-{}", std::process::id()));
        write_env_file(&path, "SECRET=\"value\"\n").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        std::fs::remove_file(path).unwrap();

        assert_eq!(mode, 0o600);
    }

    #[test]
    fn stored_value_round_trips_through_dotenv_parser() {
        let path =
            std::env::temp_dir().join(format!("ait-secrets-roundtrip-test-{}", std::process::id()));
        set_env_value(&path, "AIT_TEST_SECRET", Some("spaces and \"quotes\"")).unwrap();
        let value = read_env_value(&path, "AIT_TEST_SECRET").unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(value.as_deref(), Some("spaces and \"quotes\""));
    }

    #[test]
    fn loading_file_does_not_override_process_environment() {
        let path = std::env::temp_dir().join(format!(
            "ait-secrets-precedence-test-{}",
            std::process::id()
        ));
        write_env_file(&path, "AIT_TEST_PRECEDENCE=\"from-file\"\n").unwrap();
        std::env::set_var("AIT_TEST_PRECEDENCE", "from-process");
        load_env_file(&path).unwrap();
        let value = std::env::var("AIT_TEST_PRECEDENCE").unwrap();
        std::env::remove_var("AIT_TEST_PRECEDENCE");
        std::fs::remove_file(path).unwrap();

        assert_eq!(value, "from-process");
    }

    #[test]
    fn removing_unstored_value_does_not_create_env_file() {
        let path =
            std::env::temp_dir().join(format!("ait-secrets-absent-test-{}", std::process::id()));
        set_env_value(&path, "MISSING_KEY", None).unwrap();

        assert!(!path.exists());
    }
}
