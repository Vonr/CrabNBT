//!
//!
//! -?((0b(0|1)+|0x[0-9a-fA-F]+)|0(b|s|i|l|f|d|B|S|I|L|F|D)?|[1-9][0-9]*(b|s|i|l|f|d|B|S|I|L|F|D)?|[1-9][0-9]*.[0-9]*(f|d|F|D)?)

use std::{fmt::Debug, str::FromStr};

use crate::nbt::{
    error::SnbtDeserialisationError,
    snbt::de::utils::{expect_str, read_slice_while_skipping, ReaderAction, StrVisitor},
    NbtTag,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberType {
    Byte,
    Short,
    Integer,
    Long,
    Float,
    Double,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signedness {
    Signed,
    Unsigned,
    /// No explicit signedness was specified.
    /// This will default to [`Self::Signed`] for anything except floats and doubles,
    /// where signedness doesn't actually exist.
    // Adding unspecified was primarily a decision for Niceness.
    // We could use Signed as a default, but this would be less readable than Unspecified lit.
    // One additional variant costs no performance (assuming jump tables)
    // and is unlikely to cause any spacial costs (e.g. Option still has 253 different values)
    Unspecified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Radix {
    Binary,
    Decimal,
    Hexadecimal,
}
impl Radix {
    pub const fn check_character(self, c: char) -> bool {
        match self {
            Self::Binary => c == '0' || c == '1',
            Self::Decimal => c.is_ascii_digit() || c == '.' || c == '-' || c == 'e' || c == 'E',
            Self::Hexadecimal => c.is_ascii_hexdigit(),
        }
    }

    pub const fn get_radix_number(self) -> u32 {
        match self {
            Self::Binary => 2,
            Self::Decimal => 10,
            Self::Hexadecimal => 16,
        }
    }
}

/// Generates code to decode the signedness and number type using multi-layered match-statements.
///
/// Number suffixes are deeply layered, but often vary in minutiae depending on previous branches.
/// Creating a runtime-state for this turned out to require function pointer for basically every
/// action.
///
/// - `match_target` may run multiple times so be sure that is is sound to do so.
/// - `error_unsignable` will run at most once and do so whenever a character would have been valid
///   before a signedness suffix, but a signedness suffix was already processed, so it isn't.
/// - the tail (everything after else) may contain additional branches
///   and should usually have an error handler.
macro_rules! parse_number_suffix {
    (
        match $match_target:expr;
        all,
        error_unsignable => $error_unsignable:tt,
        $(on_false_signedness = $on_false_signedness:expr,)?
        else $($tail:tt)+
    ) => {
        parse_number_suffix!(
            match $match_target;
            b => (Signedness::Unspecified, NumberType::Byte),
            s => {
                // An "s" at the start may be either a short or the "signed" suffix.
                // The two are completely indistinguishable.
                parse_number_suffix!(
                    match $match_target;
                    post_signedness,
                    sign = Signedness::Signed,
                    error_unsignable => $error_unsignable,
                    // if nothing else matches, it must have been a short.
                    else c => {
                        $(($on_false_signedness)(c);)?
                        (Signedness::Unspecified, NumberType::Short)
                    }
                )
            },
            i => (Signedness::Unspecified, NumberType::Integer),
            l => (Signedness::Unspecified, NumberType::Long),
            // Signedness suffixes don't exist for Floats and Doubles,
            f => (Signedness::Unspecified, NumberType::Float),
            d => (Signedness::Unspecified, NumberType::Double),
            u => {
                parse_number_suffix!(
                    match $match_target;
                    post_signedness,
                    sign = Signedness::Unsigned,
                    error_unsignable => $error_unsignable,
                    // "u" only exists as a signedness suffix
                    else $($tail)+
                )
            },
            else $($tail)+
        )
    };
    (
        match $match_target:expr;
        post_signedness,
        // Signedness is pulled into this macro arm so that the inner match-statements
        // have the same return type as the outer match statement.
        // This in turn means that $tail can work in all match statements.
        // (Note that that in itself is nice as well, since only a single match arm can execute
        //  and tail always has the lowest priority)
        sign = $signedness:expr,
        error_unsignable => $error_unsignable:expr,
        else $($tail:tt)+
    ) => {
        parse_number_suffix!(
            match $match_target;
            b => ($signedness, NumberType::Byte),
            // Since signedness suffixes must prefix a number type suffix,
            // this being post_signedness(_suffix), means we are guaranteed a number type suffix.
            s => ($signedness, NumberType::Short),
            i => ($signedness, NumberType::Integer),
            l => ($signedness, NumberType::Long),
            // Floats cannot have a signedness
            f => $error_unsignable,
            d => $error_unsignable,
            // Cannot have two signedness suffixes, so "u" is out
            u => $error_unsignable,
            else $($tail)+
        )
    };
    {
        match $match_target:expr;
        $(b => $byte:expr, )?
        $(s => $short:expr, )?
        $(i => $int:expr, )?
        $(l => $long:expr, )?
        $(f => $float:expr, )?
        $(d => $double:expr, )?
        $(u => $unsigned:expr, )?
        else $($tail:tt)+
    } => {
        match $match_target {
            $(Some('b' | 'B') => $byte,)?
            $(Some('s' | 'S') => $short,)?
            $(Some('i' | 'I') => $int,)?
            $(Some('l' | 'L') => $long,)?
            $(Some('f' | 'F') => $float,)?
            $(Some('d' | 'D') => $double,)?
            $(Some('u' | 'U') => $unsigned,)?
            $($tail)+
        }
    };
}

pub fn read_number_or_numboid_const(
    visitor: &mut StrVisitor,
) -> Result<NbtTag, SnbtDeserialisationError> {
    if expect_str(visitor, "true").is_ok() {
        Ok(NbtTag::Byte(1))
    } else if expect_str(visitor, "false").is_ok() {
        Ok(NbtTag::Byte(0))
    } else {
        read_number(visitor)
    }
}

fn read_number(visitor: &mut StrVisitor) -> Result<NbtTag, SnbtDeserialisationError> {
    // TODO: Check _ implementation. May be underconstrained
    //  (no check for end/beginning of sequence, which may cause bugs)
    let mut is_float_only: bool = false;
    // What radix the number is in.
    // Along with signedness and number_type this is one of the three properties that classify
    // all possible SNBT numbers (and actually a few impossible SNBT numbers, but we Err those)
    let mut radix: Radix = Radix::Decimal;
    // Radix numbers always start with 0 as their first character.
    // Instead of testing inside read_slice_while, we can read ahead and simplify the code.
    let mut can_have_radix_prefix = visitor.peek().is_some_and(|c: char| c == '0');
    // Radices are defined at the second index, so we have to track this as well
    let mut has_read_once = false;
    let slice = &visitor.get_slice()[visitor.get_position()..];
    let mut num_end: usize = 0;
    while let Some(c) = visitor.peek() {
        if has_read_once && can_have_radix_prefix {
            match c {
                'b' => radix = Radix::Binary,
                'x' => radix = Radix::Hexadecimal,
                // Decimal case
                c if c.is_ascii_digit() || c == '.' => {}
                _ => break,
            }
            can_have_radix_prefix = false;
            num_end += 1;
            visitor.next().unwrap();
            continue;
        }
        has_read_once = true;
        match c {
            '.' => {
                is_float_only = true;
                num_end += 1;
                visitor.next().unwrap();
                continue;
            }
            'e' | 'E' if radix != Radix::Hexadecimal => {
                is_float_only = true;
                num_end += 1;
                visitor.next().unwrap();
                continue;
            }
            '_' => {
                visitor.next().unwrap();
                if visitor.peek().is_some_and(|c| c.is_ascii_digit()) {
                    num_end += 1;
                    continue;
                } else {
                    return Err(SnbtDeserialisationError::from_visitor(
                        visitor,
                        "number should come after _",
                    ));
                }
            }
            c if radix.check_character(c) => {
                num_end += 1;
                visitor.next().unwrap();
                continue;
            }
            _ => break,
        }
    }
    let num_str = &slice[..num_end];
    // match &num_str {
    //     Cow::Borrowed(b) => println!("Borrowed({b})"),
    //     Cow::Owned(o) => println!("Owned({o})")
    // }

    let (mut signedness, mut number_type) = parse_number_suffix! {
        match visitor.next();
        all,
        error_unsignable => {
            visitor.previous();
            todo!("better error structures")
        },
        on_false_signedness = |c: Option<char>| if c.is_some() { visitor.previous(); },
        else read_c => {
            // when visitor.next() returns None, it does not advance further, therefore previous()
            // would move
            if read_c.is_some() {
                // The character might be a "," or similar.
                // Higher parsers can worry about that character, I'm just creating numbers.
                visitor.previous();
            }
            if is_float_only {
                // Unmarked floating-point value defaults to double
                (Signedness::Unspecified, NumberType::Double)
            } else {
                (Signedness::Unspecified, NumberType::Integer)
            }
        }
    };

    // from_str_radix does not expect prefixes (that is "0x" and "0b")
    let without_radix_prefix = match radix {
        Radix::Binary => {
            if num_str.len() == 2 {
                // "0b" looks like the start of a binary number to the reader,
                // so we must check and perform this correction.
                (radix, signedness, number_type) =
                    (Radix::Decimal, Signedness::Unspecified, NumberType::Byte);
                "0"
            } else {
                &num_str[2..]
            }
        }
        Radix::Hexadecimal => &num_str[2..],
        Radix::Decimal => &num_str,
    };
    match get_number_parser(radix, signedness, number_type) {
        Some(parser) => parser(without_radix_prefix),
        None => Err(SnbtDeserialisationError::IllegalCombination(
            radix,
            signedness,
            number_type,
        )),
    }
}

macro_rules! wrap_float {
    ($mapper:expr) => {
        (|s| {
            FromStr::from_str(s)
                .map($mapper)
                .map_err(SnbtDeserialisationError::ParseFloatError)
        }) as for<'a> fn(&'a _) -> _
    };
}

macro_rules! wrap_int {
    ($radix:ident, $source_type:ty, $dest_type:ty) => {
        (|s| {
            <$source_type>::from_str_radix(s, Radix::$radix.get_radix_number())
                .map(|source| source as $dest_type)
                .map(NbtTag::from)
                .map_err(SnbtDeserialisationError::ParseIntError)
        }) as for<'a> fn(&'a _) -> _
    };
    ($radix:ident, $source_type:ty) => {
        wrap_int!($radix, $source_type, $source_type)
    };
}

macro_rules! find_parser_matching {
    // TODO: Implement existing combinations and add remaining ones.
    //  Every radix can appear with every signedness for every (integer) number type.
    //  This makes 3*2*4 = 24 possible combinations.
    //  Coding this all by hand would be silly, therefore I recommend making use of macros
    //  or clever generics. Consider that from_str_radix exists for _every_ integral type,
    //  so maybe some kind of Trait to make use of this would be a good solution.
    //  Consider adding the num_traits dependency for this purpose, as it already has a trait
    //  for this problem (https://docs.rs/num-traits/latest/num_traits/trait.Num.html#tymethod.from_str_radix).
    ($target:expr) => {
        find_parser_matching!(
            $target,
            integers(
                radixes = [Binary, Decimal, Hexadecimal],
                number_type = [Byte, Short, Integer, Long]
            ),
            floats(Float, Double)
        )
    };
    (
        $target:expr,
        integers(
            radixes = [$($radix:ident $(,)?)+],
            number_type = [$($int_type:ident $(,)?)+]
        ),
        floats(
            $($float_type:ident $(,)?)+
        )
    ) => {{
        macro_rules! foo {
            ($used_radix:ident, $sign:expr, $uint:ty, $sint:ty) => {
                Some(
                    match $sign {
                        Signedness::Unsigned => wrap_int!($used_radix, $uint, $sint),
                        Signedness::Signed | Signedness::Unspecified => wrap_int!($used_radix, $sint)
                    }
                )
            }
        }

        match $target {
            $((Radix::$radix, sign, NumberType::Byte) => foo!($radix, sign, u8, i8),)+
            $((Radix::$radix, sign, NumberType::Short) => foo!($radix, sign, u16, i16),)+
            $((Radix::$radix, sign, NumberType::Integer) => foo!($radix, sign, u32, i32),)+
            $((Radix::$radix, sign, NumberType::Long) => foo!($radix, sign, u64, i64),)+
            $((Radix::Decimal, Signedness::Unspecified, NumberType::$float_type) => Some(wrap_float!(NbtTag::$float_type)),)+
            _ => None
        }
    }};
}

/// Returns a function to correctly parse any given
/// combination of [`Radix`], [`Signedness`], and [`NumberType`],
/// if one exists, otherwise [`None`].
const fn get_number_parser(
    radix: Radix,
    signedness: Signedness,
    number_type: NumberType,
) -> Option<fn(&str) -> Result<NbtTag, SnbtDeserialisationError>> {
    find_parser_matching!((radix, signedness, number_type))
}

/// Whether this character may contained anywhere within a number sequence,
/// including special number formats such as hexadecimal (0xff), binary (0b101),
/// or exponential (1.0E-3).
fn is_number_character(c: char) -> bool {
    // important! Keep consistent with [`may_start_number`]
    // see https://minecraft.wiki/w/NBT_format#Number_format
    // exponential included, because E and e are both hexdigits.
    c.is_ascii_hexdigit() || c == '.' || c == '-' || c == '_' || c == 'x'
}

pub fn may_start_number(c: char) -> bool {
    c.is_ascii_digit() || c == '.' || c == '-'
}
