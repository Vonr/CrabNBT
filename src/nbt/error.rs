use std::{error::Error, fmt::Display};

use crate::nbt::snbt::de::utils::StrVisitor;

#[derive(Debug, PartialEq, Eq)]
pub struct SnbtDeserialisationError {
    pub index: usize,
    pub offending_area: String,
    pub expected: &'static str
}
const MAX_OFFENSE_INFO_LENGTH: usize = 12;
impl SnbtDeserialisationError {
    pub fn from_visitor(visitor: &StrVisitor, expected: &'static str) -> Self {
        // get at most {MAX_OFFENSE_INFO_LENGTH} characters before the offending character
        let offending_area = visitor.get_slice()[..visitor.get_position()]
            .chars()
            .rev()
            .take(MAX_OFFENSE_INFO_LENGTH)
            // this extra allocation is sadly unavoidable without significant (and bug prone) effort
            .collect::<Vec<char>>()
            .iter()
            .rev()
            .collect()
            ;
        SnbtDeserialisationError {
            index: visitor.get_position(),
            offending_area,
            expected
        }
    }
}
impl Display for SnbtDeserialisationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Expected {} at position {}: {} <--[HERE]",
            self.expected,
            self.index,
            self.offending_area
        )
    }
}
impl Error for SnbtDeserialisationError { }
