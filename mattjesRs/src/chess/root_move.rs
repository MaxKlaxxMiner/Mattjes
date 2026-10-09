use super::Move;

/// What a mate search knows about one root move after a depth: a proven mate
/// with its length in plies and line (`mate_plies` 0 with `proven` set when
/// the length is unknown, i.e. the proof tree lost entries), or an unproven
/// move. Searches return their root moves best first, so a GUI's MultiPV
/// list stays complete even before a mate is found.
#[derive(Clone, Default, Debug)]
pub struct RootMove {
    pub mv: Move,
    pub proven: bool,
    pub mate_plies: u32,
    pub pv: Vec<Move>,
}
