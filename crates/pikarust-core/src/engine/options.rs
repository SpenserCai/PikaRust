use std::fmt;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OptionError {
    #[error("unknown option: {0}")]
    UnknownOption(String),
    #[error("invalid value for {name}: {reason}")]
    InvalidValue { name: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UciOption {
    Spin {
        name: String,
        default: i64,
        min: i64,
        max: i64,
    },
    Check {
        name: String,
        default: bool,
    },
    String {
        name: String,
        default: String,
    },
    Button {
        name: String,
    },
}

impl fmt::Display for UciOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spin {
                name,
                default,
                min,
                max,
            } => write!(
                f,
                "option name {name} type spin default {default} min {min} max {max}"
            ),
            Self::Check { name, default } => {
                write!(f, "option name {name} type check default {default}")
            }
            Self::String { name, default } => {
                let val = if default.is_empty() {
                    "<empty>"
                } else {
                    default.as_str()
                };
                write!(f, "option name {name} type string default {val}")
            }
            Self::Button { name } => write!(f, "option name {name} type button"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineOptions {
    pub hash_mb: usize,
    pub threads: usize,
    pub multi_pv: usize,
    pub move_overhead: i32,
    pub ponder: bool,
    pub show_wdl: bool,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            hash_mb: 16,
            threads: 1,
            multi_pv: 1,
            move_overhead: 10,
            ponder: false,
            show_wdl: false,
        }
    }
}

const MAX_HASH_MB: i64 = 65536;
const MAX_THREADS: i64 = 1024;
const MAX_MULTI_PV: i64 = 500;
const MAX_MOVE_OVERHEAD: i64 = 5000;

impl EngineOptions {
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), OptionError> {
        match name.to_ascii_lowercase().as_str() {
            "hash" => {
                let v = parse_spin(name, value, 1, MAX_HASH_MB)?;
                self.hash_mb = v as usize;
            }
            "threads" => {
                let v = parse_spin(name, value, 1, MAX_THREADS)?;
                self.threads = v as usize;
            }
            "multipv" => {
                let v = parse_spin(name, value, 1, MAX_MULTI_PV)?;
                self.multi_pv = v as usize;
            }
            "move overhead" => {
                let v = parse_spin(name, value, 0, MAX_MOVE_OVERHEAD)?;
                self.move_overhead = v as i32;
            }
            "ponder" => {
                self.ponder = parse_check(name, value)?;
            }
            "uci_showwdl" => {
                self.show_wdl = parse_check(name, value)?;
            }
            _ => return Err(OptionError::UnknownOption(name.to_owned())),
        }
        Ok(())
    }

    pub fn uci_options() -> Vec<UciOption> {
        vec![
            UciOption::Spin {
                name: "Hash".to_owned(),
                default: 16,
                min: 1,
                max: MAX_HASH_MB,
            },
            UciOption::Spin {
                name: "Threads".to_owned(),
                default: 1,
                min: 1,
                max: MAX_THREADS,
            },
            UciOption::Spin {
                name: "MultiPV".to_owned(),
                default: 1,
                min: 1,
                max: MAX_MULTI_PV,
            },
            UciOption::Spin {
                name: "Move Overhead".to_owned(),
                default: 10,
                min: 0,
                max: MAX_MOVE_OVERHEAD,
            },
            UciOption::Check {
                name: "Ponder".to_owned(),
                default: false,
            },
            UciOption::Check {
                name: "UCI_ShowWDL".to_owned(),
                default: false,
            },
        ]
    }
}

fn parse_spin(name: &str, value: &str, min: i64, max: i64) -> Result<i64, OptionError> {
    let v: i64 = value.parse().map_err(|_| OptionError::InvalidValue {
        name: name.to_owned(),
        reason: format!("expected integer, got '{value}'"),
    })?;
    if v < min || v > max {
        return Err(OptionError::InvalidValue {
            name: name.to_owned(),
            reason: format!("value {v} out of range [{min}, {max}]"),
        });
    }
    Ok(v)
}

fn parse_check(name: &str, value: &str) -> Result<bool, OptionError> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(OptionError::InvalidValue {
            name: name.to_owned(),
            reason: format!("expected 'true' or 'false', got '{value}'"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options() {
        let opts = EngineOptions::default();
        assert_eq!(opts.hash_mb, 16);
        assert_eq!(opts.threads, 1);
        assert_eq!(opts.multi_pv, 1);
        assert_eq!(opts.move_overhead, 10);
        assert!(!opts.ponder);
        assert!(!opts.show_wdl);
    }

    #[test]
    fn test_set_hash() {
        let mut opts = EngineOptions::default();
        opts.set("Hash", "256").unwrap();
        assert_eq!(opts.hash_mb, 256);
    }

    #[test]
    fn test_set_hash_boundary() {
        let mut opts = EngineOptions::default();
        opts.set("Hash", "1").unwrap();
        assert_eq!(opts.hash_mb, 1);
        opts.set("Hash", "65536").unwrap();
        assert_eq!(opts.hash_mb, 65536);
    }

    #[test]
    fn test_set_hash_out_of_range() {
        let mut opts = EngineOptions::default();
        assert!(opts.set("Hash", "0").is_err());
        assert!(opts.set("Hash", "65537").is_err());
    }

    #[test]
    fn test_set_threads() {
        let mut opts = EngineOptions::default();
        opts.set("Threads", "8").unwrap();
        assert_eq!(opts.threads, 8);
    }

    #[test]
    fn test_set_multi_pv() {
        let mut opts = EngineOptions::default();
        opts.set("MultiPV", "3").unwrap();
        assert_eq!(opts.multi_pv, 3);
    }

    #[test]
    fn test_set_move_overhead() {
        let mut opts = EngineOptions::default();
        opts.set("Move Overhead", "100").unwrap();
        assert_eq!(opts.move_overhead, 100);
    }

    #[test]
    fn test_set_ponder() {
        let mut opts = EngineOptions::default();
        opts.set("Ponder", "true").unwrap();
        assert!(opts.ponder);
        opts.set("Ponder", "false").unwrap();
        assert!(!opts.ponder);
    }

    #[test]
    fn test_set_show_wdl() {
        let mut opts = EngineOptions::default();
        opts.set("UCI_ShowWDL", "true").unwrap();
        assert!(opts.show_wdl);
    }

    #[test]
    fn test_set_unknown_option() {
        let mut opts = EngineOptions::default();
        assert!(opts.set("UnknownOption", "42").is_err());
    }

    #[test]
    fn test_set_invalid_spin_value() {
        let mut opts = EngineOptions::default();
        assert!(opts.set("Hash", "abc").is_err());
    }

    #[test]
    fn test_set_invalid_check_value() {
        let mut opts = EngineOptions::default();
        assert!(opts.set("Ponder", "yes").is_err());
    }

    #[test]
    fn test_case_insensitive_option_name() {
        let mut opts = EngineOptions::default();
        opts.set("hash", "128").unwrap();
        assert_eq!(opts.hash_mb, 128);
        opts.set("HASH", "64").unwrap();
        assert_eq!(opts.hash_mb, 64);
    }

    #[test]
    fn test_uci_options_list() {
        let options = EngineOptions::uci_options();
        assert_eq!(options.len(), 6);

        assert!(
            matches!(&options[0], UciOption::Spin { name, default: 16, min: 1, max: 65536 } if name == "Hash")
        );
        assert!(
            matches!(&options[1], UciOption::Spin { name, default: 1, min: 1, max: 1024 } if name == "Threads")
        );
        assert!(
            matches!(&options[4], UciOption::Check { name, default: false } if name == "Ponder")
        );
    }

    #[test]
    fn test_uci_option_display_spin() {
        let opt = UciOption::Spin {
            name: "Hash".to_owned(),
            default: 16,
            min: 1,
            max: 65536,
        };
        assert_eq!(
            opt.to_string(),
            "option name Hash type spin default 16 min 1 max 65536"
        );
    }

    #[test]
    fn test_uci_option_display_check() {
        let opt = UciOption::Check {
            name: "Ponder".to_owned(),
            default: false,
        };
        assert_eq!(
            opt.to_string(),
            "option name Ponder type check default false"
        );
    }

    #[test]
    fn test_uci_option_display_string() {
        let opt = UciOption::String {
            name: "EvalFile".to_owned(),
            default: String::new(),
        };
        assert_eq!(
            opt.to_string(),
            "option name EvalFile type string default <empty>"
        );
    }

    #[test]
    fn test_uci_option_display_button() {
        let opt = UciOption::Button {
            name: "Clear Hash".to_owned(),
        };
        assert_eq!(opt.to_string(), "option name Clear Hash type button");
    }
}
