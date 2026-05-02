//! UCI command parser.

use crate::command::{BenchParams, GoParams, UciCommand};
use crate::error::UciError;

/// Parse a UCI command string into a [`UciCommand`].
pub fn parse_command(input: &str) -> Result<UciCommand, UciError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(UciError::ParseError("empty input".to_string()));
    }

    let mut tokens = input.split_whitespace();
    let cmd = tokens
        .next()
        .ok_or_else(|| UciError::ParseError("empty input".to_string()))?;

    match cmd.to_ascii_lowercase().as_str() {
        "uci" => Ok(UciCommand::Uci),
        "isready" => Ok(UciCommand::IsReady),
        "ucinewgame" => Ok(UciCommand::UciNewGame),
        "stop" => Ok(UciCommand::Stop),
        "quit" => Ok(UciCommand::Quit),
        "ponderhit" => Ok(UciCommand::PonderHit),
        "flip" => Ok(UciCommand::Flip),
        "d" => Ok(UciCommand::D),
        "debug" => parse_debug(&mut tokens),
        "position" => parse_position(&mut tokens),
        "go" => parse_go(&mut tokens),
        "setoption" => parse_set_option(&mut tokens),
        "bench" => parse_bench(&mut tokens),
        _ => Err(UciError::UnknownCommand(cmd.to_string())),
    }
}

fn parse_debug<'a>(tokens: &mut impl Iterator<Item = &'a str>) -> Result<UciCommand, UciError> {
    match tokens.next().map(str::to_ascii_lowercase).as_deref() {
        Some("on") => Ok(UciCommand::Debug(true)),
        Some("off") => Ok(UciCommand::Debug(false)),
        Some(other) => Err(UciError::ParseError(format!(
            "expected 'on' or 'off' after debug, got '{other}'"
        ))),
        None => Err(UciError::ParseError(
            "expected 'on' or 'off' after debug".to_string(),
        )),
    }
}

fn parse_position<'a>(tokens: &mut impl Iterator<Item = &'a str>) -> Result<UciCommand, UciError> {
    let first = tokens.next().ok_or_else(|| {
        UciError::InvalidPosition("expected 'startpos' or 'fen' after position".to_string())
    })?;

    let remaining: Vec<&str> = tokens.collect();

    match first.to_ascii_lowercase().as_str() {
        "startpos" => {
            let moves = extract_moves(&remaining);
            Ok(UciCommand::Position { fen: None, moves })
        }
        "fen" => {
            let (fen, moves) = extract_fen_and_moves(&remaining)?;
            Ok(UciCommand::Position {
                fen: Some(fen),
                moves,
            })
        }
        _ => Err(UciError::InvalidPosition(format!(
            "expected 'startpos' or 'fen', got '{first}'"
        ))),
    }
}

fn extract_moves(tokens: &[&str]) -> Vec<String> {
    let moves_idx = tokens.iter().position(|t| t.eq_ignore_ascii_case("moves"));

    moves_idx.map_or_else(Vec::new, |idx| {
        tokens[idx + 1..].iter().map(|s| (*s).to_string()).collect()
    })
}

fn extract_fen_and_moves(tokens: &[&str]) -> Result<(String, Vec<String>), UciError> {
    let moves_idx = tokens.iter().position(|t| t.eq_ignore_ascii_case("moves"));

    let fen_parts = moves_idx.map_or(tokens, |idx| &tokens[..idx]);

    if fen_parts.is_empty() {
        return Err(UciError::InvalidPosition(
            "missing FEN string after 'fen'".to_string(),
        ));
    }

    let fen = fen_parts.join(" ");
    let moves = moves_idx.map_or_else(Vec::new, |idx| {
        tokens[idx + 1..].iter().map(|s| (*s).to_string()).collect()
    });

    Ok((fen, moves))
}

fn parse_go<'a>(tokens: &mut impl Iterator<Item = &'a str>) -> Result<UciCommand, UciError> {
    let mut params = GoParams::default();
    let token_vec: Vec<&str> = tokens.collect();
    let mut i = 0;

    while i < token_vec.len() {
        match token_vec[i].to_ascii_lowercase().as_str() {
            "wtime" => {
                params.wtime = Some(parse_next_u64(&token_vec, &mut i)?);
            }
            "btime" => {
                params.btime = Some(parse_next_u64(&token_vec, &mut i)?);
            }
            "winc" => {
                params.winc = Some(parse_next_u64(&token_vec, &mut i)?);
            }
            "binc" => {
                params.binc = Some(parse_next_u64(&token_vec, &mut i)?);
            }
            "movestogo" => {
                params.movestogo = Some(parse_next_u32(&token_vec, &mut i)?);
            }
            "depth" => {
                params.depth = Some(parse_next_u32(&token_vec, &mut i)?);
            }
            "nodes" => {
                params.nodes = Some(parse_next_u64(&token_vec, &mut i)?);
            }
            "movetime" => {
                params.movetime = Some(parse_next_u64(&token_vec, &mut i)?);
            }
            "infinite" => params.infinite = true,
            "ponder" => params.ponder = true,
            "searchmoves" => {
                i += 1;
                while i < token_vec.len() && !is_go_keyword(token_vec[i]) {
                    params.searchmoves.push(token_vec[i].to_string());
                    i += 1;
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }

    Ok(UciCommand::Go(params))
}

fn is_go_keyword(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "wtime"
            | "btime"
            | "winc"
            | "binc"
            | "movestogo"
            | "depth"
            | "nodes"
            | "movetime"
            | "infinite"
            | "ponder"
            | "searchmoves"
    )
}

fn parse_next_u64(tokens: &[&str], i: &mut usize) -> Result<u64, UciError> {
    *i += 1;
    tokens
        .get(*i)
        .ok_or_else(|| UciError::ParseError("expected numeric value".to_string()))?
        .parse::<u64>()
        .map_err(|e| UciError::ParseError(format!("invalid number: {e}")))
}

fn parse_next_u32(tokens: &[&str], i: &mut usize) -> Result<u32, UciError> {
    *i += 1;
    tokens
        .get(*i)
        .ok_or_else(|| UciError::ParseError("expected numeric value".to_string()))?
        .parse::<u32>()
        .map_err(|e| UciError::ParseError(format!("invalid number: {e}")))
}

fn parse_set_option<'a>(
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Result<UciCommand, UciError> {
    let token_vec: Vec<&str> = tokens.collect();

    let name_idx = token_vec
        .iter()
        .position(|t| t.eq_ignore_ascii_case("name"))
        .ok_or_else(|| UciError::ParseError("expected 'name' after setoption".to_string()))?;

    let value_idx = token_vec
        .iter()
        .position(|t| t.eq_ignore_ascii_case("value"));

    let name_end = value_idx.unwrap_or(token_vec.len());
    let name = token_vec[name_idx + 1..name_end].join(" ");

    if name.is_empty() {
        return Err(UciError::ParseError("option name is empty".to_string()));
    }

    let value = value_idx.map(|vi| token_vec[vi + 1..].join(" "));

    Ok(UciCommand::SetOption { name, value })
}

fn parse_bench<'a>(tokens: &mut impl Iterator<Item = &'a str>) -> Result<UciCommand, UciError> {
    let token_vec: Vec<&str> = tokens.collect();

    if token_vec.is_empty() {
        return Ok(UciCommand::Bench(None));
    }

    let mut params = BenchParams {
        depth: None,
        threads: None,
        hash: None,
    };

    let mut i = 0;
    while i < token_vec.len() {
        match token_vec[i].to_ascii_lowercase().as_str() {
            "depth" => {
                params.depth = Some(parse_next_u32(&token_vec, &mut i)?);
            }
            "threads" => {
                params.threads = Some(parse_next_u32(&token_vec, &mut i)?);
            }
            "hash" => {
                params.hash = Some(parse_next_u32(&token_vec, &mut i)?);
            }
            _ => {}
        }
        i += 1;
    }

    Ok(UciCommand::Bench(Some(params)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uci() {
        assert_eq!(parse_command("uci").unwrap(), UciCommand::Uci);
        assert_eq!(parse_command("UCI").unwrap(), UciCommand::Uci);
        assert_eq!(parse_command("  uci  ").unwrap(), UciCommand::Uci);
    }

    #[test]
    fn test_parse_isready() {
        assert_eq!(parse_command("isready").unwrap(), UciCommand::IsReady);
        assert_eq!(parse_command("ISREADY").unwrap(), UciCommand::IsReady);
    }

    #[test]
    fn test_parse_ucinewgame() {
        assert_eq!(parse_command("ucinewgame").unwrap(), UciCommand::UciNewGame);
    }

    #[test]
    fn test_parse_stop() {
        assert_eq!(parse_command("stop").unwrap(), UciCommand::Stop);
    }

    #[test]
    fn test_parse_quit() {
        assert_eq!(parse_command("quit").unwrap(), UciCommand::Quit);
    }

    #[test]
    fn test_parse_ponderhit() {
        assert_eq!(parse_command("ponderhit").unwrap(), UciCommand::PonderHit);
    }

    #[test]
    fn test_parse_flip() {
        assert_eq!(parse_command("flip").unwrap(), UciCommand::Flip);
    }

    #[test]
    fn test_parse_d() {
        assert_eq!(parse_command("d").unwrap(), UciCommand::D);
    }

    #[test]
    fn test_parse_debug_on() {
        assert_eq!(parse_command("debug on").unwrap(), UciCommand::Debug(true));
    }

    #[test]
    fn test_parse_debug_off() {
        assert_eq!(
            parse_command("debug off").unwrap(),
            UciCommand::Debug(false)
        );
    }

    #[test]
    fn test_parse_debug_missing_arg() {
        assert!(parse_command("debug").is_err());
    }

    #[test]
    fn test_parse_debug_invalid_arg() {
        assert!(parse_command("debug maybe").is_err());
    }

    #[test]
    fn test_parse_position_startpos() {
        assert_eq!(
            parse_command("position startpos").unwrap(),
            UciCommand::Position {
                fen: None,
                moves: vec![],
            }
        );
    }

    #[test]
    fn test_parse_position_startpos_with_moves() {
        assert_eq!(
            parse_command("position startpos moves e2e4 e7e5").unwrap(),
            UciCommand::Position {
                fen: None,
                moves: vec!["e2e4".to_string(), "e7e5".to_string()],
            }
        );
    }

    #[test]
    fn test_parse_position_fen() {
        let fen = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";
        let cmd = format!("position fen {fen}");
        assert_eq!(
            parse_command(&cmd).unwrap(),
            UciCommand::Position {
                fen: Some(fen.to_string()),
                moves: vec![],
            }
        );
    }

    #[test]
    fn test_parse_position_fen_with_moves() {
        let fen = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";
        let cmd = format!("position fen {fen} moves h2e2 h9g7");
        assert_eq!(
            parse_command(&cmd).unwrap(),
            UciCommand::Position {
                fen: Some(fen.to_string()),
                moves: vec!["h2e2".to_string(), "h9g7".to_string()],
            }
        );
    }

    #[test]
    fn test_parse_position_missing_arg() {
        assert!(parse_command("position").is_err());
    }

    #[test]
    fn test_parse_position_invalid_arg() {
        assert!(parse_command("position invalid").is_err());
    }

    #[test]
    fn test_parse_position_fen_missing_fen_string() {
        assert!(parse_command("position fen").is_err());
    }

    #[test]
    fn test_parse_go_infinite() {
        assert_eq!(
            parse_command("go infinite").unwrap(),
            UciCommand::Go(GoParams {
                infinite: true,
                ..GoParams::default()
            })
        );
    }

    #[test]
    fn test_parse_go_depth() {
        assert_eq!(
            parse_command("go depth 10").unwrap(),
            UciCommand::Go(GoParams {
                depth: Some(10),
                ..GoParams::default()
            })
        );
    }

    #[test]
    fn test_parse_go_time_control() {
        assert_eq!(
            parse_command("go wtime 300000 btime 300000 winc 5000 binc 5000").unwrap(),
            UciCommand::Go(GoParams {
                wtime: Some(300_000),
                btime: Some(300_000),
                winc: Some(5000),
                binc: Some(5000),
                ..GoParams::default()
            })
        );
    }

    #[test]
    fn test_parse_go_movetime() {
        assert_eq!(
            parse_command("go movetime 5000").unwrap(),
            UciCommand::Go(GoParams {
                movetime: Some(5000),
                ..GoParams::default()
            })
        );
    }

    #[test]
    fn test_parse_go_nodes() {
        assert_eq!(
            parse_command("go nodes 1000000").unwrap(),
            UciCommand::Go(GoParams {
                nodes: Some(1_000_000),
                ..GoParams::default()
            })
        );
    }

    #[test]
    fn test_parse_go_ponder() {
        assert_eq!(
            parse_command("go ponder wtime 300000 btime 300000").unwrap(),
            UciCommand::Go(GoParams {
                ponder: true,
                wtime: Some(300_000),
                btime: Some(300_000),
                ..GoParams::default()
            })
        );
    }

    #[test]
    fn test_parse_go_searchmoves() {
        assert_eq!(
            parse_command("go searchmoves e2e4 d2d4 depth 10").unwrap(),
            UciCommand::Go(GoParams {
                searchmoves: vec!["e2e4".to_string(), "d2d4".to_string()],
                depth: Some(10),
                ..GoParams::default()
            })
        );
    }

    #[test]
    fn test_parse_go_movestogo() {
        assert_eq!(
            parse_command("go wtime 60000 btime 60000 movestogo 30").unwrap(),
            UciCommand::Go(GoParams {
                wtime: Some(60_000),
                btime: Some(60_000),
                movestogo: Some(30),
                ..GoParams::default()
            })
        );
    }

    #[test]
    fn test_parse_go_empty() {
        assert_eq!(
            parse_command("go").unwrap(),
            UciCommand::Go(GoParams::default())
        );
    }

    #[test]
    fn test_parse_setoption_with_value() {
        assert_eq!(
            parse_command("setoption name Hash value 128").unwrap(),
            UciCommand::SetOption {
                name: "Hash".to_string(),
                value: Some("128".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_setoption_multi_word_name() {
        assert_eq!(
            parse_command("setoption name Skill Level value 10").unwrap(),
            UciCommand::SetOption {
                name: "Skill Level".to_string(),
                value: Some("10".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_setoption_button() {
        assert_eq!(
            parse_command("setoption name Clear Hash").unwrap(),
            UciCommand::SetOption {
                name: "Clear Hash".to_string(),
                value: None,
            }
        );
    }

    #[test]
    fn test_parse_setoption_missing_name() {
        assert!(parse_command("setoption").is_err());
    }

    #[test]
    fn test_parse_setoption_empty_name() {
        assert!(parse_command("setoption name value 10").is_err());
    }

    #[test]
    fn test_parse_bench_no_params() {
        assert_eq!(parse_command("bench").unwrap(), UciCommand::Bench(None));
    }

    #[test]
    fn test_parse_bench_with_params() {
        assert_eq!(
            parse_command("bench depth 16 threads 4 hash 256").unwrap(),
            UciCommand::Bench(Some(BenchParams {
                depth: Some(16),
                threads: Some(4),
                hash: Some(256),
            }))
        );
    }

    #[test]
    fn test_parse_empty_input() {
        assert!(parse_command("").is_err());
        assert!(parse_command("   ").is_err());
    }

    #[test]
    fn test_parse_unknown_command() {
        let err = parse_command("foobar").unwrap_err();
        assert!(matches!(err, UciError::UnknownCommand(_)));
    }

    #[test]
    fn test_parse_extra_whitespace() {
        assert_eq!(
            parse_command("  go   depth   10  ").unwrap(),
            UciCommand::Go(GoParams {
                depth: Some(10),
                ..GoParams::default()
            })
        );
    }

    #[test]
    fn test_parse_case_insensitive_commands() {
        assert_eq!(parse_command("UCI").unwrap(), UciCommand::Uci);
        assert_eq!(parse_command("IsReady").unwrap(), UciCommand::IsReady);
        assert_eq!(parse_command("STOP").unwrap(), UciCommand::Stop);
        assert_eq!(parse_command("Quit").unwrap(), UciCommand::Quit);
        assert_eq!(parse_command("D").unwrap(), UciCommand::D);
    }

    #[test]
    fn test_parse_go_invalid_number() {
        assert!(parse_command("go depth abc").is_err());
    }

    #[test]
    fn test_parse_go_missing_number() {
        assert!(parse_command("go depth").is_err());
    }

    #[test]
    fn test_parse_position_fen_moves_keyword_only() {
        let fen = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";
        let cmd = format!("position fen {fen} moves");
        assert_eq!(
            parse_command(&cmd).unwrap(),
            UciCommand::Position {
                fen: Some(fen.to_string()),
                moves: vec![],
            }
        );
    }

    #[test]
    fn test_parse_position_startpos_moves_keyword_only() {
        assert_eq!(
            parse_command("position startpos moves").unwrap(),
            UciCommand::Position {
                fen: None,
                moves: vec![],
            }
        );
    }

    #[test]
    fn test_go_params_default() {
        let params = GoParams::default();
        assert_eq!(params.wtime, None);
        assert_eq!(params.btime, None);
        assert_eq!(params.winc, None);
        assert_eq!(params.binc, None);
        assert_eq!(params.movestogo, None);
        assert_eq!(params.depth, None);
        assert_eq!(params.nodes, None);
        assert_eq!(params.movetime, None);
        assert!(!params.infinite);
        assert!(!params.ponder);
        assert!(params.searchmoves.is_empty());
    }
}
