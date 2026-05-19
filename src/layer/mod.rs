pub mod ai;
pub mod failure;
pub mod history;
pub mod history_import;
pub mod risk;
pub mod specs;

use crate::protocol::RequestContext;

pub struct SuggestInput<'a> {
    pub input: &'a str,
    pub cursor_pos: usize,
    pub context: &'a RequestContext,
}
