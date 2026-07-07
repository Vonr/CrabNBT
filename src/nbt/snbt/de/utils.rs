use std::{borrow::Cow, fmt::Debug, iter::FusedIterator, result::Result as StdResult};

use crate::nbt::error::SnbtDeserialisationError;

pub type Result<T> = std::result::Result<T, SnbtDeserialisationError>;

/// A middleman trait for [`std::str::FromStr`].
/// 
pub trait FromVisitor: Sized {
    type Err;

    fn from_visitor(visitor: &mut StrVisitor) -> std::result::Result<Self, Self::Err>;
}

/// Provides an implementation of [`std::str::FromStr`] for types that implement [`FromVisitor`].
/// It calls [`FromVisitor::from_visitor`] and if, after the method returns,
/// the visitor is not exhausted, returns an [`Err`] variant (otherwise, returns the result)
macro_rules! impl_FromStr_through_FromVisitor {
    ($type:ty) => {
        impl std::str::FromStr for $type {
            type Err = <$type as FromVisitor>::Err;

            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                let mut visitor = StrVisitor::new(s);
                Self::from_visitor(&mut visitor)
                    .and_then(|res| {
                        match visitor.peek() {
                            None => Ok(res),
                            Some(_) => Err(SnbtDeserialisationError::from_visitor(&visitor, "EOF"))
                        }
                    })
            }
        }
    };
}
pub(crate) use impl_FromStr_through_FromVisitor;

/// A more flexible replacement for [`std::str::Chars`].
/// Allows character-by-character iteration over a string,
/// but with built-in support for peeking and teeing (via clone())
#[must_use]
pub struct StrVisitor<'a> {
    slice: &'a str,
    /// Position offset to the start of the slice (in bytes)
    position: usize,
}
impl<'a> StrVisitor<'a> {
    /// Creates a new visitor over the given slice, starting at position 0.
    pub fn new(slice: &'a str) -> Self {
        StrVisitor { slice, position: 0 }
    }

    /// Returns the next char in the visitor without advancing.
    /// 
    /// It is guaranteed that the result of this method 
    /// will be the same as the next result of [`Self::next`]
    pub fn peek(&self) -> Option<char> {
        self.as_str().chars().next()
    }

    /// Moves back to the last read character.
    pub fn previous(&mut self) -> Option<char> {
        self.position = previous_char_boundary(self.slice, self.position);
        self.peek()
    }

    pub fn next_if<P: FnOnce(char) -> bool>(&mut self, predicate: P) -> Option<char> {
        self.peek().filter(|c| predicate(*c)).inspect(|_| { self.next(); })
    }

    /// Try to map the next character using `mapper`,
    /// returning the result and only advancing the visitor if `mapper` returned a [`Some`].
    pub fn next_try_map<T, F: FnOnce(Option<char>) -> Option<T>>(&mut self, mapper: F) -> Option<T> {
        mapper(self.peek()).inspect(|_| { self.next(); })
    }

    /// Read at most n characters in this visitor and return both the resulting slice
    /// and the amount of characters in that slice.
    /// 
    /// Note that, if less than n characters remain before the visitor ends,
    /// this method will return the entire slice read before the visitor ended.
    /// If you need exactly n characters in your string, make sure to check the returned number
    /// matches your expected value.
    pub fn next_n(&mut self, n: usize) -> (&'a str, usize) {
        let start_position = self.position;
        let characters_read = (1..=n)
            .take_while(|_| self.next().is_some())
            .last()
            .unwrap_or(0);
        // no panic on rangeindex here, because next guarantees posiiton being at a char boundary.
        (&self.slice[start_position..self.position], characters_read)
    }

    /// Advances the visitor by `amount` characters or until the end of the slice.
    /// 
    /// Returns the amount of times the visitor advanced.
    pub fn advance_by(&mut self, amount: usize) -> usize {
        let mut advances = 0usize;
        for _ in 0..amount {
            if self.next().is_some() {
                advances += 1;
            }
        }
        advances
    }

    /// Returns the remaining part of the slice
    pub fn as_str(&self) -> &'a str {
        &self.slice[self.position..]
    }

    pub fn get_position(&self) -> usize {
        return self.position
    }

    pub fn get_slice(&self) -> &'a str {
        self.slice
    }

    /// Returns the string slice after ahead and before self,
    /// if ahead is advanced at least as far as self and both refer to the same string slice.
    /// 
    /// If ahead is behind self, returns [`None`].
    /// 
    /// # Panics
    /// ...if self and ahead point to different string, even if one is a substring of the other.
    pub fn get_slice_up_to(&self, ahead: &StrVisitor<'a>) -> Option<&'a str> {
        if self.slice != ahead.slice {
            panic!("get_slice_up_to called on visitors to different strings");
        } else if ahead.position < self.position {
            None
        } else {
            // No panic: ahead.position >= self.position (checked above)
            //  StrVisitor always ensures that position is on a char boundary
            Some(&self.slice[self.position..ahead.position])
        }
    }
}
impl<'a> Clone for StrVisitor<'a> {
    /// Creates another visitor, backed by the same slice at the current position
    fn clone(&self) -> Self {
        StrVisitor {
            slice: self.slice,
            position: self.position,
        }
    }
}
impl<'a> Iterator for StrVisitor<'a> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.peek();
        if result.is_some() {
            self.position = next_char_boundary(self.slice, self.position);
        }
        result
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.slice.chars().size_hint();
        (lower - self.position, upper.map(|i| i - self.position))
    }
}
impl<'a> FusedIterator for StrVisitor<'a> {}

/// Finds the next char boundary greater than index. Saturates to slice.len()
///
/// This can be replaced once [`str::ceil_char_boundary`] is stabilised 
/// (requiring Rust 2025 Edition).
/// see rust-lang/rust#93743 for more
fn next_char_boundary(slice: &str, index: usize) -> usize {
    for i in (index + 1)..slice.len() {
        if slice.is_char_boundary(i) {
            return i;
        }
    }
    slice.len()
}

/// Finds the last char boundary less than index. Saturates to 0
/// 
/// Like [`next_char_boundary`] this can be replaced
/// once [`str::floor_char_boundary`] is stabilised.
fn previous_char_boundary(slice: &str, index: usize) -> usize {
    for i in (0..index).rev() {
        if slice.is_char_boundary(i) {
            return i;
        }
    }
    0
}

pub(crate) fn expect_char(
    visitor: &mut StrVisitor,
    expected_char: char,
    expected: &'static str
) -> Result<()> {
    expect_condition(visitor, &|c| c == expected_char, expected).map(|_| ())
}

/// Returns [`Ok`] only if the visitor continues with a string matching `expected`.
/// If [`Ok`] is returned the visitor will be advanced until exactly after that substring.
/// 
/// If the visitor does not continue with `expected`,
/// returns an [`Err`] and the visitor is not advanced at all.
pub(crate) fn expect_str(
    visitor: &mut StrVisitor,
    expected: &'static str
) -> Result<()> {
    if visitor.as_str().starts_with(expected) {
        visitor.position += expected.len();
        Ok(())
    } else {
        Err(SnbtDeserialisationError::from_visitor(visitor, expected))
    }
}

pub(crate) fn expect_any_literal(
    visitor: &mut StrVisitor,
    expected: &mut [(&mut dyn Iterator<Item = char>, fn())]
) -> usize {
    let matches = 0usize;
    let mut possible_values = vec![false; expected.len()];
    for c in visitor {
        for ((seq, fun), is_possible) in expected.iter_mut()
            .zip(possible_values.iter_mut())
            .filter(|(_, b)| **b)
        {
            match seq.next() {
                None => {
                    fun();
                    *is_possible = false;
                },
                Some(x) => {
                    if x != c {
                        *is_possible = false
                    }
                }
            }
        }
    }
    matches
}

pub(crate) fn expect_condition<'a, P: FnOnce(char) -> bool>(
    visitor: &mut StrVisitor,
    predicate: P,
    expected: &'static str,
) -> Result<char> {
    match visitor.next() {
        Some(x) if predicate(x) => Ok(x),
        _ => Err(SnbtDeserialisationError::from_visitor(visitor, expected)),
    }
}

pub(crate) fn consume_while<P>(visitor: &mut StrVisitor, mut condition: P)
    where P: FnMut(char) -> bool
{
    while let Some(future) = visitor.peek() {
        if !condition(future) {
            break;
        }
        visitor.next();
    }
}

pub(crate) fn consume_whitespace(visitor: &mut StrVisitor) {
    consume_while(visitor, |c| c.is_whitespace())
}

pub(crate) fn read_slice_while<'a, P>(visitor: &mut StrVisitor<'a>, condition: P) -> &'a str
    where P: FnMut(char) -> bool
{
    let start = visitor.clone();
    consume_while(visitor, condition);
    start.get_slice_up_to(visitor)
        // TODO: Remove this sketchy expect
        .expect("visitor should be further advanced than start in read_slice_while, but isn't")
}

/// Helper type for [`read_slice_while_skipping`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReaderAction {
    Accept,
    Skip,
    Abort
}

/// Like [`read_slice_while`], but allows skipping characters.
/// Note that if a character is skipped, a new allocation is created to contain the (now modified)
/// version of the [`String`].
pub(crate) fn read_slice_while_skipping<'a, P>(
    visitor: &mut StrVisitor<'a>,
    mut condition: P
) -> Cow<'a, str>
    where P: FnMut(char) -> ReaderAction
{
    let mut break_was_skip: bool = false;
    let starting_slice = read_slice_while(
        visitor,
        |c| {
            match condition(c) {
                ReaderAction::Accept => true,
                ReaderAction::Abort => false,
                ReaderAction::Skip => {
                    break_was_skip = true;
                    false
                }
            }
        }
    );
    if break_was_skip {
        let mut working_string = starting_slice.to_owned();
        while let Some(read_char) = visitor.peek() {
            match condition(read_char) {
                ReaderAction::Accept => {
                    working_string.push(read_char);
                    visitor.next();
                },
                ReaderAction::Skip => { visitor.next(); },
                ReaderAction::Abort => {
                    break
                }
            }
        }
        Cow::Owned(working_string)
    } else {
        Cow::Borrowed(starting_slice)
    }
}

pub(crate) fn read_string(visitor: &mut StrVisitor) -> Result<String> {
    let first_char = visitor.peek()
        .ok_or(SnbtDeserialisationError::from_visitor(
            visitor,
            "quote or any unquotable character"
        ))?;
    match first_char {
        '"' | '\'' => read_quoted_string(visitor),
        _ => read_unquoted_string(visitor)
    }
}

pub(crate) fn read_quoted_string(visitor: &mut StrVisitor) -> Result<String> {
    let mut result = String::new();
    let quote_char = expect_condition(
        visitor,
        |c| c == '"' || c == '\'',
        "a quote character"
    )?;

    while let Some(c) = visitor.next() {
        if c == quote_char {
            result.shrink_to_fit();
            return Ok(result);
        } else if c == '\\' {
            result.push(parse_escape_sequence(visitor)?);
        } else {
            result.push(c);
        }
    }
    Err(SnbtDeserialisationError::from_visitor(visitor, "a quote character"))
}

/// Reads an unquoted SNBT string value. 
/// Returns an [`Err`] variant if the first character doesn't match, 
/// placing `visitor` over that character.
/// 
/// This algorithm is greedy, meaning it will consume all valid characters in a sequence.
pub(crate) fn read_unquoted_string(visitor: &mut StrVisitor) -> Result<String> {
    const START_EXPECTED: &'static str = "one of [a-zA-z]|-|\\+|\\.";
    match visitor.peek() {
        Some(c) if char_may_start_unquoted(c) => { },
        _ => return Err(SnbtDeserialisationError::from_visitor(visitor, START_EXPECTED))
    }
    
    let mut result = String::new();
    while let Some(c) = visitor.next_if(char_may_be_unquoted) {
        // escape sequences are not allowed in unquoted strings
        result.push(c)
    }
    Ok(result)
}

pub(crate) fn char_may_be_unquoted(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '+' || c == '.' || c == '_'
}
pub(crate) fn char_may_start_unquoted(c: char) -> bool {
    // TODO: I think something went missing here.
    // There should be some character may _not_ start an unquoted string
    c.is_ascii_alphabetic()
}

/// read and evaluate an SNBT escape sequence.
/// Assumes the \ character was already read.
fn parse_escape_sequence(visitor: &mut StrVisitor) -> Result<char> {
    const ESCAPABLE_CHARACTER: &'static str = "any escapable character";
    let escaped_char = visitor
        .next()
        .ok_or_else(|| SnbtDeserialisationError::from_visitor(
            &visitor,
            ESCAPABLE_CHARACTER
        ))?;
    // see https://minecraft.wiki/w/NBT_format#Escape_sequences
    Ok(match escaped_char {
        '\\' => '\\',
        '\'' => '\'',
        '"' => '"',
        'b' => '\x08', // backspace
        'f' => '\x0C', // formfeed
        'n' => '\n',
        'r' => '\r',
        's' => ' ',
        't' => '\t',
        'x' => {
            parse_hexadecimal_as_char(visitor, u8::from_str_radix, 2)?
        }
        'u' => {
            parse_hexadecimal_as_char(
                visitor,
                |s, r| u16::from_str_radix(s, r).map(|i| i as u32),
                4
            )?
        }
        'U' => {
            parse_hexadecimal_as_char(visitor, u32::from_str_radix, 8)?
        }
        'N' => {
            expect_char(visitor, '{', "{")?;
            let c = parse_unicode_name(visitor)?;
            expect_char(visitor, '}', "}")?;
            c
        }
        _ => {
            return Err(SnbtDeserialisationError::from_visitor(
                visitor,
                ESCAPABLE_CHARACTER
            ))
        }
    })
}

fn parse_hexadecimal_as_char<N: TryInto<char, Error = E2>, E2, E, M>(
    visitor: &mut StrVisitor,
    parser: M,
    num_expected: usize
) -> Result<char>
    where M: FnOnce(&str, u32) -> StdResult<N, E>,
        // TODO: Remove this once better error structures is done
        E: Debug, E2: Debug
{
    let (hex, chars) = visitor.next_n(num_expected);
    if chars != num_expected {
        Err(SnbtDeserialisationError::from_visitor(
            visitor,
            // TODO: This error message could use improvement
            "C hexadecimal characters"
        ))
    } else {
        Ok(
            parser(hex, 16)
                .expect("todo: better error structures")
                .try_into()
                .expect("todo: better error structures")
        )
    }
}

fn parse_unicode_name(visitor: &mut StrVisitor) -> Result<char> {
    let mut name = String::new();
    while let Some(c) = visitor.peek() {
        if matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | ' ') {
            name.push(visitor.next().unwrap());
        } else {
            break;
        }
    }

    if visitor.peek().is_none_or(|c| c != '}') {
        Err(SnbtDeserialisationError::from_visitor(
            visitor,
            // TODO: This error message could use improvement
            "expected '}' while reading Unicode name from \\N escape code",
        ))
    } else {
        unicode_names2::character(&name).ok_or(SnbtDeserialisationError::from_visitor(
            visitor,
            "symbol matching this name was not found",
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::nbt::snbt::de::utils::{parse_escape_sequence, StrVisitor};

    #[test]
    fn escape_sequences() {
        fn parse_escape_with_str(slice: &str) -> char {
            parse_escape_sequence(&mut StrVisitor::new(slice)).unwrap()
        }
        assert_eq!(parse_escape_with_str("n"), '\n');
        assert_eq!(parse_escape_with_str("r"), '\r');
    }
}
