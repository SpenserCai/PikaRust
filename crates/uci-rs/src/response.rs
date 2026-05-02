//! UCI response types representing messages from the engine to the GUI.

use std::fmt;

/// Score information in a UCI `info` response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Score {
    Cp(i32),
    Mate(i32),
    Lowerbound(i32),
    Upperbound(i32),
}

impl fmt::Display for Score {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cp(cp) => write!(f, "score cp {cp}"),
            Self::Mate(m) => write!(f, "score mate {m}"),
            Self::Lowerbound(cp) => write!(f, "score cp {cp} lowerbound"),
            Self::Upperbound(cp) => write!(f, "score cp {cp} upperbound"),
        }
    }
}

/// Parameters for the `info` response.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InfoParams {
    pub depth: Option<u32>,
    pub seldepth: Option<u32>,
    pub time: Option<u64>,
    pub nodes: Option<u64>,
    pub pv: Option<Vec<String>>,
    pub score: Option<Score>,
    pub wdl: Option<(i32, i32, i32)>,
    pub currmove: Option<String>,
    pub currmovenumber: Option<u32>,
    pub hashfull: Option<u32>,
    pub nps: Option<u64>,
    pub string: Option<String>,
}

/// UCI option definition sent during the `uci` handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionDef {
    Check {
        name: String,
        default: bool,
    },
    Spin {
        name: String,
        default: i64,
        min: i64,
        max: i64,
    },
    Combo {
        name: String,
        default: String,
        options: Vec<String>,
    },
    Button {
        name: String,
    },
    Str {
        name: String,
        default: String,
    },
}

impl fmt::Display for OptionDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Check { name, default } => {
                write!(f, "option name {name} type check default {default}")
            }
            Self::Spin {
                name,
                default,
                min,
                max,
            } => {
                write!(
                    f,
                    "option name {name} type spin default {default} min {min} max {max}"
                )
            }
            Self::Combo {
                name,
                default,
                options,
            } => {
                write!(f, "option name {name} type combo default {default}")?;
                for opt in options {
                    write!(f, " var {opt}")?;
                }
                Ok(())
            }
            Self::Button { name } => write!(f, "option name {name} type button"),
            Self::Str { name, default } => {
                write!(f, "option name {name} type string default {default}")
            }
        }
    }
}

/// A UCI response from the engine to the GUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UciResponse {
    Id {
        name: String,
        author: String,
    },
    UciOk,
    ReadyOk,
    BestMove {
        best: String,
        ponder: Option<String>,
    },
    Info(InfoParams),
    Option(OptionDef),
}

impl fmt::Display for UciResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id { name, author } => {
                write!(f, "id name {name}\nid author {author}")
            }
            Self::UciOk => write!(f, "uciok"),
            Self::ReadyOk => write!(f, "readyok"),
            Self::BestMove { best, ponder } => {
                write!(f, "bestmove {best}")?;
                if let Some(p) = ponder {
                    write!(f, " ponder {p}")?;
                }
                Ok(())
            }
            Self::Info(params) => {
                write!(f, "info")?;
                if let Some(d) = params.depth {
                    write!(f, " depth {d}")?;
                }
                if let Some(sd) = params.seldepth {
                    write!(f, " seldepth {sd}")?;
                }
                if let Some(t) = params.time {
                    write!(f, " time {t}")?;
                }
                if let Some(n) = params.nodes {
                    write!(f, " nodes {n}")?;
                }
                if let Some(score) = &params.score {
                    write!(f, " {score}")?;
                }
                if let Some((w, d, l)) = params.wdl {
                    write!(f, " wdl {w} {d} {l}")?;
                }
                if let Some(cm) = &params.currmove {
                    write!(f, " currmove {cm}")?;
                }
                if let Some(cmn) = params.currmovenumber {
                    write!(f, " currmovenumber {cmn}")?;
                }
                if let Some(hf) = params.hashfull {
                    write!(f, " hashfull {hf}")?;
                }
                if let Some(nps) = params.nps {
                    write!(f, " nps {nps}")?;
                }
                if let Some(pv) = &params.pv {
                    if !pv.is_empty() {
                        write!(f, " pv")?;
                        for m in pv {
                            write!(f, " {m}")?;
                        }
                    }
                }
                if let Some(s) = &params.string {
                    write!(f, " string {s}")?;
                }
                Ok(())
            }
            Self::Option(opt) => write!(f, "{opt}"),
        }
    }
}
