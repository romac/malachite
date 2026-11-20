use derive_where::derive_where;
use displaydoc::Display;
use time::OffsetDateTime;

use malachitebft_core_types::{Height, Round};

use super::Line;

#[derive_where(Clone, Debug, Eq, PartialEq)]
#[derive(Display)]
#[displaydoc("[{time}] height: {height}, round: {round}, line: {line}")]
pub struct Trace<H: Height> {
    pub time: OffsetDateTime,
    pub height: H,
    pub round: Round,
    pub line: Line,
}

impl<H: Height> Trace<H> {
    pub fn new(height: H, round: Round, line: Line) -> Self {
        Self {
            time: OffsetDateTime::now_utc(),
            height,
            round,
            line,
        }
    }
}
