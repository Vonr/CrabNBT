use std::num::{ParseFloatError, ParseIntError};

use crate::nbt::snbt::de::{
    numbers::{NumberType, Radix, Signedness},
    utils::StrVisitor,
};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum SnbtDeserialisationError {
    #[error("Illegal combination of {0:?}, {1:?}, and {2:?}")]
    IllegalCombination(Radix, Signedness, NumberType),
    #[error(transparent)]
    ParseFloatError(ParseFloatError),
    #[error(transparent)]
    ParseIntError(ParseIntError),

    #[error("Expected {expected} at position {index}: {offending_area} <--[HERE]")]
    Unexpected {
        index: usize,
        offending_area: String,
        expected: &'static str,
    },
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
            .collect();
        SnbtDeserialisationError::Unexpected {
            index: visitor.get_position(),
            offending_area,
            expected,
        }
    }
}
