//! Secrets, kept out of the configuration file so the file can be shared
//! and edited freely. The file refers to a secret by name,
//! `{secret: home_assistant_token}`. Its value comes from the environment
//! variable `DUCKBOARD_SECRET_HOME_ASSISTANT_TOKEN` when that is set, and
//! otherwise from the secrets file: `secrets.yaml` beside the config unless
//! it says otherwise, a map of names to values, git-ignored and best
//! readable by its owner alone.

use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A reference to a secret by name.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SecretRef {
    /// The secret's name in the secrets file; the environment variable
    /// DUCKBOARD_SECRET_<NAME> overrides it.
    pub secret: String,
}

/// The environment variable that overrides a secret: its name in capitals,
/// anything but letters and digits made an underscore.
pub fn env_name(name: &str) -> String {
    let tail: String =
        name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
    format!("DUCKBOARD_SECRET_{tail}")
}

/// The secrets file, read the first time a secret is not in the
/// environment.
pub struct Secrets {
    path: PathBuf,
    values: Option<IndexMap<String, String>>,
}

impl Secrets {
    pub fn new(path: PathBuf) -> Self {
        Self { path, values: None }
    }

    pub fn get(&mut self, r: &SecretRef) -> Result<String> {
        self.resolve(&r.secret, |k| std::env::var(k).ok())
    }

    fn resolve(&mut self, name: &str, env: impl Fn(&str) -> Option<String>) -> Result<String> {
        let var = env_name(name);
        if let Some(v) = env(&var) {
            return Ok(v);
        }
        let values = match &mut self.values {
            Some(v) => v,
            None => self.values.insert(read(&self.path).with_context(|| format!("secret {name} (or set {var})"))?),
        };
        match values.get(name) {
            Some(v) => Ok(v.clone()),
            None => bail!("secret {name}: not in {}, and {var} is not set", self.path.display()),
        }
    }
}

fn read(path: &Path) -> Result<IndexMap<String, String>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    warn_if_shared(path);
    serde_yaml_ng::from_str(&text).with_context(|| format!("parsing {}: a map of names to values", path.display()))
}

/// Says so when others than the owner may read the file.
fn warn_if_shared(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = std::fs::metadata(path) {
            if m.permissions().mode() & 0o077 != 0 {
                eprintln!("duckboard: {} can be read by others; chmod 600 it", path.display());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets(yaml: &str) -> Secrets {
        let dir = std::env::temp_dir().join(format!("duckboard-secrets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("s{}.yaml", yaml.len()));
        std::fs::write(&path, yaml).unwrap();
        Secrets::new(path)
    }

    #[test]
    fn names_become_variables() {
        assert_eq!(env_name("home_assistant_token"), "DUCKBOARD_SECRET_HOME_ASSISTANT_TOKEN");
        assert_eq!(env_name("ha-token.2"), "DUCKBOARD_SECRET_HA_TOKEN_2");
    }

    #[test]
    fn the_environment_wins_over_the_file() {
        let mut s = secrets("a: from-file\n");
        assert_eq!(s.resolve("a", |_| None).unwrap(), "from-file");
        let env = |k: &str| (k == "DUCKBOARD_SECRET_A").then(|| "from-env".to_string());
        assert_eq!(s.resolve("a", env).unwrap(), "from-env");
    }

    #[test]
    fn a_missing_secret_says_where_it_looked() {
        let mut s = secrets("a: x\n");
        let err = format!("{:#}", s.resolve("b", |_| None).unwrap_err());
        assert!(err.contains("DUCKBOARD_SECRET_B") && err.contains(".yaml"), "{err}");
        let mut none = Secrets::new("/nonexistent/secrets.yaml".into());
        assert_eq!(none.resolve("b", |k| Some(k.to_string())).unwrap(), "DUCKBOARD_SECRET_B", "no file needed");
        assert!(none.resolve("b", |_| None).is_err());
    }
}
