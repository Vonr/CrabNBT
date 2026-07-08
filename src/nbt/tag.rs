use bytes::{Buf, BufMut, Bytes, BytesMut};
use crab_nbt::error::Error;
use crab_nbt::nbt::compound::NbtCompound;
use crab_nbt::nbt::utils::*;
use derive_more::From;
use std::fmt::{self, Debug, Display, Formatter};
use std::io::Cursor;
use std::mem::{discriminant, Discriminant};

use crate::nbt::error::SnbtDeserialisationError;
use crate::nbt::snbt::de::numbers::{may_start_number, read_number_or_numboid_const, NumberType};
use crate::nbt::snbt::de::utils::{
    consume_whitespace, expect_char, expect_str, expect_str_ignore_case,
    impl_FromStr_through_FromVisitor, read_slice_while, read_string, FromVisitor, StrVisitor,
};

use crate::nbt::list::NbtList;

/// Enum representing the different types of NBT tags.
/// Each variant corresponds to a different type of data that can be stored in an NBT tag.
#[repr(u8)]
#[derive(Clone, Debug, PartialEq, PartialOrd, From)]
pub enum NbtTag {
    End = END_ID,
    Byte(i8) = BYTE_ID,
    Short(i16) = SHORT_ID,
    Int(i32) = INT_ID,
    Long(i64) = LONG_ID,
    Float(f32) = FLOAT_ID,
    Double(f64) = DOUBLE_ID,
    ByteArray(Bytes) = BYTE_ARRAY_ID,
    String(String) = STRING_ID,
    List(NbtList) = LIST_ID,
    Compound(NbtCompound) = COMPOUND_ID,
    IntArray(Vec<i32>) = INT_ARRAY_ID,
    LongArray(Vec<i64>) = LONG_ARRAY_ID,
}

impl NbtTag {
    /// Returns the numeric id associated with the data type.
    pub const fn get_type_id(&self) -> u8 {
        // See https://doc.rust-lang.org/reference/items/enumerations.html#pointer-casting
        unsafe { *(self as *const Self as *const u8) }
    }

    pub fn serialize(&self) -> Bytes {
        let mut bytes = BytesMut::new();
        bytes.put_u8(self.get_type_id());
        bytes.put(self.serialize_data());
        bytes.freeze()
    }

    pub fn serialize_data(&self) -> Bytes {
        let mut bytes = BytesMut::new();
        match self {
            NbtTag::End => {}
            NbtTag::Byte(byte) => bytes.put_i8(*byte),
            NbtTag::Short(short) => bytes.put_i16(*short),
            NbtTag::Int(int) => bytes.put_i32(*int),
            NbtTag::Long(long) => bytes.put_i64(*long),
            NbtTag::Float(float) => bytes.put_f32(*float),
            NbtTag::Double(double) => bytes.put_f64(*double),
            NbtTag::ByteArray(byte_array) => {
                bytes.put_i32(byte_array.len() as i32);
                bytes.put_slice(byte_array);
            }
            NbtTag::String(string) => {
                let java_string = simd_cesu8::encode(string);
                bytes.put_u16(java_string.len() as u16);
                bytes.put_slice(&java_string);
            }
            NbtTag::List(list) => {
                bytes.put_u8(list.element_type_id());
                bytes.put_i32(list.len() as i32);
                for tag in list.as_inner() {
                    bytes.put(tag.serialize_data())
                }
            }
            NbtTag::Compound(compound) => {
                bytes.put(compound.serialize_content());
            }
            NbtTag::IntArray(int_array) => {
                bytes.put_i32(int_array.len() as i32);
                for int in int_array {
                    bytes.put_i32(*int)
                }
            }
            NbtTag::LongArray(long_array) => {
                bytes.put_i32(long_array.len() as i32);
                for long in long_array {
                    bytes.put_i64(*long)
                }
            }
        }
        bytes.freeze()
    }

    pub fn deserialize(bytes: &mut impl Buf) -> Result<NbtTag, Error> {
        let tag_id = bytes.get_u8();
        Self::deserialize_data(bytes, tag_id)
    }

    pub fn deserialize_from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<NbtTag, Error> {
        Self::deserialize(cursor)
    }

    pub fn deserialize_data(bytes: &mut impl Buf, tag_id: u8) -> Result<NbtTag, Error> {
        match tag_id {
            END_ID => Ok(NbtTag::End),
            BYTE_ID => {
                let byte = bytes.get_i8();
                Ok(NbtTag::Byte(byte))
            }
            SHORT_ID => {
                let short = bytes.get_i16();
                Ok(NbtTag::Short(short))
            }
            INT_ID => {
                let int = bytes.get_i32();
                Ok(NbtTag::Int(int))
            }
            LONG_ID => {
                let long = bytes.get_i64();
                Ok(NbtTag::Long(long))
            }
            FLOAT_ID => {
                let float = bytes.get_f32();
                Ok(NbtTag::Float(float))
            }
            DOUBLE_ID => {
                let double = bytes.get_f64();
                Ok(NbtTag::Double(double))
            }
            BYTE_ARRAY_ID => {
                let len = bytes.get_i32() as usize;
                let byte_array = bytes.copy_to_bytes(len);
                Ok(NbtTag::ByteArray(byte_array))
            }
            STRING_ID => Ok(NbtTag::String(get_nbt_string(bytes).unwrap())),
            LIST_ID => {
                let tag_type_id = bytes.get_u8();
                let len = bytes.get_i32();
                let list = std::iter::repeat_n((), len as usize)
                    .map(|_| NbtTag::deserialize_data(bytes, tag_type_id))
                    .collect::<Result<Vec<_>, _>>()?
                    .into();
                Ok(NbtTag::List(list))
            }
            COMPOUND_ID => Ok(NbtTag::Compound(NbtCompound::deserialize_content(bytes)?)),
            INT_ARRAY_ID => {
                const BYTES: usize = size_of::<i32>();

                let len = bytes.get_i32() as usize;
                let numbers = read_array::<i32, BYTES, _>(bytes, len, i32::from_be_bytes);
                Ok(NbtTag::IntArray(numbers))
            }
            LONG_ARRAY_ID => {
                const BYTES: usize = size_of::<i64>();

                let len = bytes.get_i32() as usize;
                let numbers = read_array::<i64, BYTES, _>(bytes, len, i64::from_be_bytes);
                Ok(NbtTag::LongArray(numbers))
            }
            _ => Err(Error::UnknownTagId(tag_id)),
        }
    }

    pub fn deserialize_data_from_cursor(
        cursor: &mut Cursor<&[u8]>,
        tag_id: u8,
    ) -> Result<NbtTag, Error> {
        Self::deserialize_data(cursor, tag_id)
    }

    pub fn extract_byte(&self) -> Option<i8> {
        match self {
            NbtTag::Byte(byte) => Some(*byte),
            _ => None,
        }
    }

    pub fn extract_short(&self) -> Option<i16> {
        match self {
            NbtTag::Short(short) => Some(*short),
            _ => None,
        }
    }

    pub fn extract_int(&self) -> Option<i32> {
        match self {
            NbtTag::Int(int) => Some(*int),
            _ => None,
        }
    }

    pub fn extract_long(&self) -> Option<i64> {
        match self {
            NbtTag::Long(long) => Some(*long),
            _ => None,
        }
    }

    pub fn extract_float(&self) -> Option<f32> {
        match self {
            NbtTag::Float(float) => Some(*float),
            _ => None,
        }
    }

    pub fn extract_double(&self) -> Option<f64> {
        match self {
            NbtTag::Double(double) => Some(*double),
            _ => None,
        }
    }

    pub fn extract_bool(&self) -> Option<bool> {
        match self {
            NbtTag::Byte(byte) => Some(*byte != 0),
            _ => None,
        }
    }

    pub fn extract_byte_array(&self) -> Option<Bytes> {
        match self {
            // Note: Bytes are free to clone, so we can hand out an owned type
            NbtTag::ByteArray(byte_array) => Some(byte_array.clone()),
            _ => None,
        }
    }

    pub fn extract_string(&self) -> Option<&String> {
        match self {
            NbtTag::String(string) => Some(string),
            _ => None,
        }
    }

    pub fn extract_list(&self) -> Option<&NbtList> {
        match self {
            NbtTag::List(list) => Some(list),
            _ => None,
        }
    }

    pub fn extract_compound(&self) -> Option<&NbtCompound> {
        match self {
            NbtTag::Compound(compound) => Some(compound),
            _ => None,
        }
    }

    pub fn extract_int_array(&self) -> Option<&Vec<i32>> {
        match self {
            NbtTag::IntArray(int_array) => Some(int_array),
            _ => None,
        }
    }

    pub fn extract_long_array(&self) -> Option<&Vec<i64>> {
        match self {
            NbtTag::LongArray(long_array) => Some(long_array),
            _ => None,
        }
    }

    pub(crate) fn is_truthy(&self) -> bool {
        use self::NbtTag::*;
        match self {
            End => false,
            &Byte(x) => x != 0,
            &Short(x) => x != 0,
            &Int(x) => x != 0,
            &Long(x) => x != 0,
            &Float(x) => x <= -1.0 || x >= 1.0,
            &Double(x) => x <= -1.0 || x >= 1.0,
            ByteArray(x) => !x.is_empty(),
            IntArray(x) => !x.is_empty(),
            LongArray(x) => !x.is_empty(),
            String(x) => !x.is_empty(),
            List(x) => !x.is_empty(),
            Compound(x) => !x.child_tags.is_empty(),
        }
    }
}

impl From<&str> for NbtTag {
    fn from(value: &str) -> Self {
        NbtTag::String(value.to_string())
    }
}

impl From<&[u8]> for NbtTag {
    fn from(value: &[u8]) -> Self {
        NbtTag::ByteArray(Bytes::copy_from_slice(value))
    }
}

impl From<bool> for NbtTag {
    fn from(value: bool) -> Self {
        NbtTag::Byte(value as i8)
    }
}

impl Display for NbtTag {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::End => Ok(()),
            Self::Byte(x) => write!(f, "{x}b"),
            Self::Short(x) => write!(f, "{x}s"),
            Self::Int(x) => write!(f, "{x}"),
            Self::Long(x) => write!(f, "{x}L"),
            // using debug here matches Minecraft on whole numbers (3.0 instead of 3)
            Self::Float(x) => write!(f, "{x:?}f"),
            Self::Double(x) => write!(f, "{x:?}d"),
            Self::ByteArray(arr) => write_listlike(f, "B; ", "B", arr.iter().map(|b| *b as i8)),
            Self::String(s) => write!(f, "{}", escape_string_value(s)),
            Self::List(list) => write_listlike(f, "", "", list),
            Self::Compound(compound) => write!(f, "{compound}"),
            Self::IntArray(arr) => write_listlike(f, "I; ", "", arr),
            Self::LongArray(arr) => write_listlike(f, "L; ", "L", arr),
        }
    }
}

impl FromVisitor for NbtTag {
    type Err = SnbtDeserialisationError;

    fn from_visitor(visitor: &mut StrVisitor) -> Result<Self, Self::Err> {
        const TRUE: NbtTag = NbtTag::Byte(1);
        const FALSE: NbtTag = NbtTag::Byte(0);

        match visitor
            .peek()
            .ok_or_else(|| SnbtDeserialisationError::from_visitor(visitor, "any SNBT character"))?
        {
            '{' => NbtCompound::from_visitor(visitor).map(NbtTag::Compound),
            '[' => {
                _ = visitor.next();
                consume_whitespace(visitor);
                if visitor.next_if(|c| c == ']').is_some() {
                    Ok(NbtTag::List(NbtList::new()))
                } else if visitor.peek_nth(1).filter(|c| *c == ';').is_some() {
                    // we know visitor.nth(1) will be Some and StrVisitor is fused
                    // => visitor.next must be Some
                    match visitor.next().unwrap() {
                        'I' => read_snbt_array::<i32>(visitor).map(NbtTag::IntArray),
                        'B' => read_snbt_array::<i8>(visitor).map(|arr| {
                            NbtTag::ByteArray(arr.into_iter().map(|b| b as u8).collect())
                        }),
                        'L' => read_snbt_array::<i64>(visitor).map(NbtTag::LongArray),
                        _ => {
                            return Err(SnbtDeserialisationError::from_visitor(
                                visitor,
                                "an array type identifier",
                            ))
                        }
                    }
                } else {
                    // NOTE: This branch also triggers if visitor is fully consumed
                    read_list(visitor).map(|v| NbtTag::List(v))
                }
            }
            c if may_start_number(c) => read_number_or_numboid_const(visitor),
            _ => {
                if expect_str_ignore_case(visitor, "true").is_ok() {
                    Ok(TRUE)
                } else if expect_str_ignore_case(visitor, "false").is_ok() {
                    Ok(FALSE)
                } else if expect_str(visitor, "bool(").is_ok() {
                    consume_whitespace(visitor);

                    let tag = if visitor.peek().is_some() {
                        read_number_or_numboid_const(visitor)?.is_truthy().into()
                    } else {
                        return Err(SnbtDeserialisationError::from_visitor(visitor, "EOF"));
                    };
                    // todo!("Read number or read true/false");
                    consume_whitespace(visitor);
                    expect_char(visitor, ')', ")")?;
                    Ok(tag)
                } else if expect_str(visitor, "uuid(").is_ok() {
                    consume_whitespace(visitor);
                    let tag = uuid_from_str(&read_string(visitor)?)
                        .map(Vec::from)
                        .map(NbtTag::IntArray);

                    consume_whitespace(visitor);
                    expect_char(visitor, ')', ")")?;
                    tag
                } else {
                    read_string(visitor).map(NbtTag::String)
                }
            }
        }
    }
}
impl_FromStr_through_FromVisitor!(NbtTag);

fn write_listlike<T: Display, I: IntoIterator<Item = T>>(
    f: &mut Formatter<'_>,
    prefix: &'static str,
    affix: &'static str,
    arr: I,
) -> fmt::Result {
    write!(f, "[{prefix}")?;
    join_formatted(
        f,
        ", ",
        arr.into_iter()
            .map(|x| move |f: &mut Formatter<'_>| write!(f, "{x}{affix}")),
    )?;
    write!(f, "]")
}

fn uuid_from_str(s: &str) -> Result<[i32; 4], SnbtDeserialisationError> {
    const UUID_V4_SIZE: usize = 16;
    type Out = i32;

    let mut bytes = [0u8; UUID_V4_SIZE];
    for (i, res) in s
        .split('-')
        .flat_map(|substr| {
            (0..(substr.len() / 2)).map(|i| &substr[2 * i..(2 * (i + 1)).min(substr.len())])
        })
        .map(|s| u8::from_str_radix(s, 16))
        .enumerate()
    {
        if i >= bytes.len() {
            return Err(SnbtDeserialisationError::UuidTooManyBytes);
        }
        bytes[i] = res.map_err(SnbtDeserialisationError::ParseIntError)?;
    }

    let chunks = bytes.as_chunks::<{ size_of::<Out>() }>().0;
    let parts: [Out; UUID_V4_SIZE / size_of::<Out>()] =
        std::array::from_fn(|i| Out::from_be_bytes(chunks[i]));

    Ok(parts)
}

const LIST_SEPARATOR_MSG: &'static str = ", or ]";
/// Reads an SNBT List with correction for heterogeneous lists.
/// Assumes the opening `[` character has already been consumed.
fn read_list(visitor: &mut StrVisitor) -> Result<NbtList, SnbtDeserialisationError> {
    let mut content: Vec<NbtTag> = Vec::new();
    loop {
        consume_whitespace(visitor);
        let tag = NbtTag::from_visitor(visitor)?;
        content.push(tag);

        consume_whitespace(visitor);

        if visitor
            .next_if(|c| c == ',' || c == ']')
            .ok_or_else(|| SnbtDeserialisationError::from_visitor(visitor, LIST_SEPARATOR_MSG))?
            == ']'
            || visitor.next_if(|c| c == ']').is_some()
        {
            break;
        }
    }

    let content = content.into();
    Ok(content)
}

fn read_snbt_array<Number>(
    visitor: &mut StrVisitor,
) -> Result<Vec<Number>, SnbtDeserialisationError>
where
    Number: PrimitiveNumber + From<i8> + TryFrom<i16> + TryFrom<i32> + TryFrom<i64>,
    SnbtDeserialisationError: From<<Number as TryFrom<i16>>::Error>
        + From<<Number as TryFrom<i32>>::Error>
        + From<<Number as TryFrom<i64>>::Error>,
{
    consume_whitespace(visitor);
    // This was missing once. It took me days to find that bug!
    expect_char(visitor, ';', ";")?;
    let mut content: Vec<Number> = vec![];
    loop {
        consume_whitespace(visitor);
        if visitor.peek() != Some(']') {
            let num = crate::nbt::snbt::de::numbers::read_number(
                visitor,
                Some(Number::number_type()),
                None,
            )?;
            match num {
                NbtTag::Byte(num) => content.push(num.into()),
                NbtTag::Short(num) => {
                    if Number::bits() >= i16::bits() {
                        content.push(num.try_into()?)
                    } else {
                        return Err(SnbtDeserialisationError::from_visitor(visitor, "byte"));
                    }
                }
                NbtTag::Int(num) => {
                    if Number::bits() >= i32::bits() {
                        content.push(num.try_into()?)
                    } else {
                        return Err(SnbtDeserialisationError::from_visitor(
                            visitor,
                            "short or byte",
                        ));
                    }
                }
                NbtTag::Long(num) => {
                    if Number::bits() >= i64::bits() {
                        content.push(num.try_into()?)
                    } else {
                        return Err(SnbtDeserialisationError::from_visitor(
                            visitor,
                            "int, short, or byte",
                        ));
                    }
                }
                _ => return Err(SnbtDeserialisationError::from_visitor(visitor, "number")),
            }
        }

        consume_whitespace(visitor);
        if visitor
            .next_if(|c| c == ',' || c == ']')
            .ok_or_else(|| SnbtDeserialisationError::from_visitor(visitor, LIST_SEPARATOR_MSG))?
            == ']'
            || visitor.next_if(|c| c == ']').is_some()
        {
            break;
        }
    }
    Ok(content)
}

trait PrimitiveNumber: Sized {
    fn bits() -> u32;
    fn number_type() -> NumberType;
}

impl PrimitiveNumber for i8 {
    fn bits() -> u32 {
        Self::BITS
    }

    fn number_type() -> NumberType {
        NumberType::Byte
    }
}

impl PrimitiveNumber for i16 {
    fn bits() -> u32 {
        Self::BITS
    }

    fn number_type() -> NumberType {
        NumberType::Short
    }
}

impl PrimitiveNumber for i32 {
    fn bits() -> u32 {
        Self::BITS
    }

    fn number_type() -> NumberType {
        NumberType::Integer
    }
}

impl PrimitiveNumber for i64 {
    fn bits() -> u32 {
        Self::BITS
    }

    fn number_type() -> NumberType {
        NumberType::Long
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        nbt::{snbt::de::utils::StrVisitor, tag::read_number_or_numboid_const},
        NbtTag,
    };

    fn number_helper(s: &str) -> NbtTag {
        read_number_or_numboid_const(&mut StrVisitor::new(s)).unwrap()
    }

    #[test]
    fn test_numbers() {
        assert_eq!(number_helper("1.5"), NbtTag::Double(1.5));
    }
}
