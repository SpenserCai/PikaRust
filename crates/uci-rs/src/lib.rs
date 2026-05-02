//! UCI (Universal Chess Interface) protocol library for Chinese chess (Xiangqi).
//!
//! Provides parsing of UCI commands from the GUI and formatting of engine
//! responses. Independent of any specific engine implementation.

#![forbid(unsafe_code)]

pub mod command;
pub mod error;
pub mod parser;
pub mod response;

pub use command::{BenchParams, GoParams, UciCommand};
pub use error::UciError;
pub use parser::parse_command;
pub use response::{InfoParams, OptionDef, Score, UciResponse};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_id() {
        let resp = UciResponse::Id {
            name: "PikaRust 0.1".to_string(),
            author: "PikaRust Team".to_string(),
        };
        assert_eq!(
            resp.to_string(),
            "id name PikaRust 0.1\nid author PikaRust Team"
        );
    }

    #[test]
    fn test_response_uciok() {
        assert_eq!(UciResponse::UciOk.to_string(), "uciok");
    }

    #[test]
    fn test_response_readyok() {
        assert_eq!(UciResponse::ReadyOk.to_string(), "readyok");
    }

    #[test]
    fn test_response_bestmove() {
        let resp = UciResponse::BestMove {
            best: "e2e4".to_string(),
            ponder: None,
        };
        assert_eq!(resp.to_string(), "bestmove e2e4");
    }

    #[test]
    fn test_response_bestmove_with_ponder() {
        let resp = UciResponse::BestMove {
            best: "e2e4".to_string(),
            ponder: Some("e7e5".to_string()),
        };
        assert_eq!(resp.to_string(), "bestmove e2e4 ponder e7e5");
    }

    #[test]
    fn test_response_info_depth_score_pv() {
        let resp = UciResponse::Info(InfoParams {
            depth: Some(12),
            seldepth: Some(18),
            time: Some(1500),
            nodes: Some(2_000_000),
            score: Some(Score::Cp(35)),
            nps: Some(1_333_333),
            pv: Some(vec!["e2e4".to_string(), "e7e5".to_string()]),
            ..InfoParams::default()
        });
        assert_eq!(
            resp.to_string(),
            "info depth 12 seldepth 18 time 1500 nodes 2000000 score cp 35 nps 1333333 pv e2e4 e7e5"
        );
    }

    #[test]
    fn test_response_info_mate_score() {
        let resp = UciResponse::Info(InfoParams {
            depth: Some(20),
            score: Some(Score::Mate(3)),
            ..InfoParams::default()
        });
        assert_eq!(resp.to_string(), "info depth 20 score mate 3");
    }

    #[test]
    fn test_response_info_lowerbound() {
        let resp = UciResponse::Info(InfoParams {
            score: Some(Score::Lowerbound(100)),
            ..InfoParams::default()
        });
        assert_eq!(resp.to_string(), "info score cp 100 lowerbound");
    }

    #[test]
    fn test_response_info_upperbound() {
        let resp = UciResponse::Info(InfoParams {
            score: Some(Score::Upperbound(-50)),
            ..InfoParams::default()
        });
        assert_eq!(resp.to_string(), "info score cp -50 upperbound");
    }

    #[test]
    fn test_response_info_string() {
        let resp = UciResponse::Info(InfoParams {
            string: Some("NNUE evaluation".to_string()),
            ..InfoParams::default()
        });
        assert_eq!(resp.to_string(), "info string NNUE evaluation");
    }

    #[test]
    fn test_response_info_currmove() {
        let resp = UciResponse::Info(InfoParams {
            currmove: Some("e2e4".to_string()),
            currmovenumber: Some(1),
            ..InfoParams::default()
        });
        assert_eq!(resp.to_string(), "info currmove e2e4 currmovenumber 1");
    }

    #[test]
    fn test_response_info_hashfull() {
        let resp = UciResponse::Info(InfoParams {
            hashfull: Some(500),
            ..InfoParams::default()
        });
        assert_eq!(resp.to_string(), "info hashfull 500");
    }

    #[test]
    fn test_response_option_check() {
        let resp = UciResponse::Option(OptionDef::Check {
            name: "Ponder".to_string(),
            default: true,
        });
        assert_eq!(
            resp.to_string(),
            "option name Ponder type check default true"
        );
    }

    #[test]
    fn test_response_option_spin() {
        let resp = UciResponse::Option(OptionDef::Spin {
            name: "Hash".to_string(),
            default: 16,
            min: 1,
            max: 33_554_432,
        });
        assert_eq!(
            resp.to_string(),
            "option name Hash type spin default 16 min 1 max 33554432"
        );
    }

    #[test]
    fn test_response_option_combo() {
        let resp = UciResponse::Option(OptionDef::Combo {
            name: "Style".to_string(),
            default: "Normal".to_string(),
            options: vec![
                "Solid".to_string(),
                "Normal".to_string(),
                "Risky".to_string(),
            ],
        });
        assert_eq!(
            resp.to_string(),
            "option name Style type combo default Normal var Solid var Normal var Risky"
        );
    }

    #[test]
    fn test_response_option_button() {
        let resp = UciResponse::Option(OptionDef::Button {
            name: "Clear Hash".to_string(),
        });
        assert_eq!(resp.to_string(), "option name Clear Hash type button");
    }

    #[test]
    fn test_response_option_string() {
        let resp = UciResponse::Option(OptionDef::Str {
            name: "NalimovPath".to_string(),
            default: "/path/to/tb".to_string(),
        });
        assert_eq!(
            resp.to_string(),
            "option name NalimovPath type string default /path/to/tb"
        );
    }

    #[test]
    fn test_roundtrip_parse_and_format() {
        let cmd = parse_command("go depth 10 wtime 300000 btime 300000").unwrap();
        if let UciCommand::Go(params) = cmd {
            assert_eq!(params.depth, Some(10));
            assert_eq!(params.wtime, Some(300_000));
            assert_eq!(params.btime, Some(300_000));
        } else {
            panic!("expected Go command");
        }
    }

    #[test]
    fn test_info_empty() {
        let resp = UciResponse::Info(InfoParams::default());
        assert_eq!(resp.to_string(), "info");
    }

    #[test]
    fn test_info_pv_empty_vec() {
        let resp = UciResponse::Info(InfoParams {
            pv: Some(vec![]),
            ..InfoParams::default()
        });
        assert_eq!(resp.to_string(), "info");
    }
}
