use bytes::{Buf, BufMut, Bytes, BytesMut};
use crab_nbt::error::Error;
use crab_nbt::nbt::compound::NbtCompound;
use crab_nbt::nbt::utils::*;
use derive_more::From;
use std::fmt::{self, Debug, Display, Formatter};
use std::io::Cursor;
use std::mem::{Discriminant, discriminant};
use std::str::FromStr;

use crate::nbt::snbt::de::numbers::{may_start_number, read_number_or_numboid_const};
use crate::nbt::snbt::de::utils::{FromVisitor, StrVisitor, consume_whitespace, expect_char, expect_str, impl_FromStr_through_FromVisitor, read_slice_while, read_string};
use crate::nbt::error::SnbtDeserialisationError;

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
    List(Vec<NbtTag>) = LIST_ID,
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
                bytes.put_u8(list.first().unwrap_or(&NbtTag::End).get_type_id());
                bytes.put_i32(list.len() as i32);
                for nbt_tag in list {
                    bytes.put(nbt_tag.serialize_data())
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
                let mut list = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    let tag = NbtTag::deserialize_data(bytes, tag_type_id)?;
                    assert_eq!(tag.get_type_id(), tag_type_id);
                    list.push(tag);
                }
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

    pub fn extract_list(&self) -> Option<&Vec<NbtTag>> {
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
            Compound(x) => !x.child_tags.is_empty()
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

        match visitor.peek().ok_or_else(
            || SnbtDeserialisationError::from_visitor(visitor, "any SNBT character")
        )? {
            '{' => NbtCompound::from_visitor(visitor).map(NbtTag::Compound),
            '[' => {
                _ = visitor.next();
                consume_whitespace(visitor);
                if visitor.next_if(|c| c == ']').is_some() {
                    Ok(NbtTag::List(vec![]))
                } else if visitor.as_str().chars().nth(1).filter(|c| *c == ';').is_some() {
                    // we know visitor.nth(1) will be Some and StrVisitor is fused
                    // => visitor.next must be Some
                    match visitor.next().unwrap() {
                        'I' => read_snbt_array::<i32>(visitor).map(NbtTag::IntArray),
                        'B' => read_snbt_array::<i8>(visitor).map(
                            |arr| NbtTag::ByteArray(arr.into_iter().map(|b| b as u8).collect())
                        ),
                        'L' => read_snbt_array::<i64>(visitor).map(NbtTag::LongArray),
                        _ => return Err(SnbtDeserialisationError::from_visitor(
                            visitor,
                            "an array type identifier"
                        ))
                    }
                } else {
                    // NOTE: This branch also triggers if visitor is fully consumed
                    read_list(visitor).map(NbtTag::List)
                }
            },
            c if may_start_number(c) => read_number_or_numboid_const(visitor),
            _ => {
                if expect_str(visitor, "true").is_ok() {
                    Ok(TRUE)
                } else if expect_str(visitor, "false").is_ok() {
                    Ok(FALSE)
                } else if expect_str(visitor, "bool(").is_ok() {
                    consume_whitespace(visitor);
                    
                    let tag = if visitor.peek().is_some() {
                        read_number_or_numboid_const(visitor)?.is_truthy().into()
                    } else {
                        return Err(SnbtDeserialisationError::from_visitor(visitor, "EOF"))
                    };
                    // todo!("Read number or read true/false");
                    consume_whitespace(visitor);
                    expect_char(visitor, ')', ")")?;
                    Ok(tag)
                } else if expect_str(visitor, "uuid(").is_ok() {
                    consume_whitespace(visitor);
                    let tag = uuid_from_str(&read_string(visitor)?)
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

fn uuid_from_str(s: &str) -> Result<Vec<i32>, SnbtDeserialisationError> {
    const UUID_V4_SIZE: usize = 16;
    type Out = i32;

    let mut bytes = [0u8; UUID_V4_SIZE];
    for (i, res) in s.split('-')
        .flat_map(|substr| {
            (0..(substr.len() / 2))
                .map(|i| &substr[2*i..(2*(i+1)).min(substr.len())])
        })
        .map(|s| u8::from_str_radix(s, 16))
        .enumerate()
    {
        if i >= bytes.len() {
            todo!("better error structures");
        }
        bytes[i] = res.expect("todo: better error structures");
    }
    // Transmuting here is preferable over producing the slices manually
    // (e.g. bytes[0..4], bytes[4..8] etc.), because as of Rust 2024,
    // RangeIndexing an array gives a slice, whose size is unknown at compile time.
    // i32::from_be_bytes requires moving an array into it to work,
    // so we would either have to copy the array or use a different method (of which I am not aware)
    // 
    // SAFETY:
    //  Arrays are purely groups of data whose size is determined at compile time.
    //  More specifically, an array [T;N]'s size is determined by size_of::<T>()*N
    //      and an element arr[i] is offset by i*size_of::<T>() bytes.
    //  This means that [[T;N];M] has the same layout as a [T;N*M] with respect to its values.
    //  Ergo, transmuting here is safe, because we don't change the bounds of any elements.
    let parts: [[u8; size_of::<Out>()]; UUID_V4_SIZE/size_of::<Out>()] = unsafe { std::mem::transmute(bytes) };
    Ok(
        parts.into_iter()
            .map(|int_bytes| Out::from_be_bytes(int_bytes))
            .collect()
    )
}


const LIST_SEPARATOR_MSG: &'static str = ", or ]";
/// Reads an SNBT List with correction for heterogeneous lists.
/// Assumes the opening `[` character has already been consumed.
fn read_list(visitor: &mut StrVisitor) -> Result<Vec<NbtTag>, SnbtDeserialisationError> {
    let mut content = vec![];
    let mut homogeneous_content_type: Option<Option<Discriminant<NbtTag>>> = None;
    loop {
        consume_whitespace(visitor);
        let tag = NbtTag::from_visitor(visitor)?;
        homogeneous_content_type = homogeneous_content_type.map_or(
            Some(Some(discriminant(&tag))),
            |list_content| 
            Some(list_content.filter(|d| *d == discriminant(&tag)))
        );
        content.push(tag);

        consume_whitespace(visitor);

        if visitor.next_if(|c| c == ',' || c == ']')
            .ok_or_else(||
                SnbtDeserialisationError::from_visitor(visitor, LIST_SEPARATOR_MSG)
            )? == ']'
        {
            break
        }
    }
    if homogeneous_content_type.unwrap_or(Some(discriminant(&NbtTag::End))).is_none() {
        // Heterogeneous List correction
        // Nbt does not allow hetereogeneous lists
        // (lists where each element may have a different type),
        // but SNBT does (since https://www.minecraft.net/en-us/article/minecraft-snapshot-25w09a).
        // 
        // Heterogenous lists must be deserialised into lists of type NbtCompound,
        // with each element contained in a compound like so {"": element}
        content = content.into_iter().map(|elem| {
            [(String::new(), elem)].into_iter().collect::<NbtCompound>().into()
        }).collect();
    } else {
        content.shrink_to_fit();
    }
    Ok(content)
}

/// Reads an SNBT array of type Number,
/// assuming the array identifier (e.g. `[I;`) has already been consumed, save for the semicolon.
fn read_snbt_array<Number>(
    visitor: &mut StrVisitor
) -> Result<Vec<Number>, SnbtDeserialisationError>
    where Number: FromStr,
        // TODO: Remove me after better error structures
        Number::Err: std::error::Error
{
    consume_whitespace(visitor);
    // This was missing once. It took me days to find that bug!
    expect_char(visitor, ';', ";")?;
    let mut content = vec![];
    loop {
        consume_whitespace(visitor);
        content.push(
            read_slice_while(visitor, |c| c.is_ascii_digit() || c == '-')
                .parse()
                .expect("todo: better error structures")
        );

        consume_whitespace(visitor);
        if visitor.next_if(|c| c == ',' || c == ']').ok_or_else(
            || SnbtDeserialisationError::from_visitor(visitor, LIST_SEPARATOR_MSG)
        )? == ']' {
            break
        }
    }
    Ok(content)
}

#[cfg(test)]
mod tests {
    use crate::{NbtTag, nbt::{snbt::de::utils::StrVisitor, tag::read_number_or_numboid_const}};

    fn number_helper(s: &str) -> NbtTag {
        read_number_or_numboid_const(&mut StrVisitor::new(s)).unwrap()
    }

    #[test]
    fn test_numbers() {
        assert_eq!(number_helper("1.5"), NbtTag::Double(1.5));
    }
}
