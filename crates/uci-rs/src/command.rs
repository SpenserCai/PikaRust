//! UCI command types representing messages from the GUI to the engine.

/// Parameters for the `go` command.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoParams {
    pub wtime: Option<u64>,
    pub btime: Option<u64>,
    pub winc: Option<u64>,
    pub binc: Option<u64>,
    pub movestogo: Option<u32>,
    pub depth: Option<u32>,
    pub nodes: Option<u64>,
    pub movetime: Option<u64>,
    pub infinite: bool,
    pub ponder: bool,
    pub searchmoves: Vec<String>,
}

/// Parameters for the `bench` command (Pikafish extension).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchParams {
    pub depth: Option<u32>,
    pub threads: Option<u32>,
    pub hash: Option<u32>,
}

/// A parsed UCI command from the GUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UciCommand {
    Uci,
    IsReady,
    UciNewGame,
    Position {
        fen: Option<String>,
        moves: Vec<String>,
    },
    Go(GoParams),
    Stop,
    Quit,
    SetOption {
        name: String,
        value: Option<String>,
    },
    PonderHit,
    Debug(bool),
    Bench(Option<BenchParams>),
    Flip,
    D,
}
