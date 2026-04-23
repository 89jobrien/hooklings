//! Layered TOML configuration for hooklings.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("TOML parse error in {path}: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub pipeline: PipelineConfig,
    #[serde(default)]
    pub checks: ChecksConfig,
    #[serde(default)]
    pub emit: EmitConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub default: String,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        Self {
            default: format!("{home}/.config/hooklings/default.crux"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChecksConfig {
    #[serde(default)]
    pub op_auth: OpAuthConfig,
    #[serde(default)]
    pub ssh_reachable: SshConfig,
    #[serde(default)]
    pub handoff_pending: HandoffConfig,
    #[serde(default)]
    pub doob_pending: DoobConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpAuthConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "SshConfig::default_host")]
    pub host: String,
}

impl SshConfig {
    fn default_host() -> String {
        "minibox".into()
    }
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "minibox".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffConfig {
    pub db: String,
}

impl Default for HandoffConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        Self {
            db: format!("{home}/.local/share/atelier/handoff.db"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoobConfig {
    pub db: String,
}

impl Default for DoobConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        Self {
            db: format!("{home}/.local/share/doob/doob.db"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitConfig {
    pub json_path: String,
}

impl Default for EmitConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        Self {
            json_path: format!("{home}/.local/share/hooklings/last-preflight.json"),
        }
    }
}

impl Config {
    pub fn load_from_file(path: &Path) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&raw).map_err(|source| ConfigError::Toml {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn load() -> Self {
        let global = Self::global_path();
        let project = Self::project_path();

        let base = if global.exists() {
            Self::load_from_file(&global).unwrap_or_default()
        } else {
            Self::default()
        };

        if let Some(proj_path) = project.filter(|p| p.exists()) {
            let overlay = Self::load_from_file(&proj_path).unwrap_or_default();
            base.merge(overlay)
        } else {
            base
        }
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            pipeline: if other.pipeline.default != PipelineConfig::default().default {
                other.pipeline
            } else {
                self.pipeline
            },
            checks: ChecksConfig {
                op_auth: OpAuthConfig {
                    enabled: if other.checks.op_auth.enabled {
                        true
                    } else {
                        self.checks.op_auth.enabled
                    },
                },
                ssh_reachable: SshConfig {
                    enabled: other.checks.ssh_reachable.enabled
                        || self.checks.ssh_reachable.enabled,
                    host: if other.checks.ssh_reachable.host != SshConfig::default().host {
                        other.checks.ssh_reachable.host
                    } else {
                        self.checks.ssh_reachable.host
                    },
                },
                handoff_pending: HandoffConfig {
                    db: if other.checks.handoff_pending.db != HandoffConfig::default().db {
                        other.checks.handoff_pending.db
                    } else {
                        self.checks.handoff_pending.db
                    },
                },
                doob_pending: DoobConfig {
                    db: if other.checks.doob_pending.db != DoobConfig::default().db {
                        other.checks.doob_pending.db
                    } else {
                        self.checks.doob_pending.db
                    },
                },
            },
            emit: if other.emit.json_path != EmitConfig::default().json_path {
                other.emit
            } else {
                self.emit
            },
        }
    }

    fn global_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join(".config/hooklings/hooklings.toml")
    }

    fn project_path() -> Option<PathBuf> {
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let candidate = dir.join(".hooklings.toml");
            if candidate.exists() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }
}
