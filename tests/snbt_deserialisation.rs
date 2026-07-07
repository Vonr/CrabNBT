use std::assert_matches;
use std::str::FromStr as _;

use crab_nbt::{nbt, NbtCompound, NbtList, NbtTag};

macro_rules! assert_parse {
    ($input:expr, $expected:expr) => {
        assert_eq!($input.parse(), Ok($expected))
    };
}

macro_rules! assert_parse_failure {
    ($input:expr) => {
        ::std::assert_matches!($input.parse::<NbtTag>(), Err(_))
    };
    ($input:expr, $err:expr) => {
        assert_eq!($input.parse::<NbtTag>(), Err($err))
    };
}

fn tag_helper(s: &str) -> NbtTag {
    NbtTag::from_str(s).unwrap()
}

fn wrap(tag: NbtTag) -> NbtTag {
    NbtCompound {
        child_tags: vec![(String::new(), tag)],
    }
    .into()
}

#[test]
fn nbt_tag() {
    assert_eq!(tag_helper("Hello"), NbtTag::String("Hello".to_owned()));
}

#[test]
fn nbt_list() {
    assert_eq!(tag_helper("[]"), NbtTag::List(NbtList::new()));
    assert_eq!(
        tag_helper("[A,B,C ,D,      E,    F    ,   G    ]"),
        NbtTag::List(
            "ABCDEFG"
                .chars()
                .map(|c| NbtTag::String(c.to_string()))
                .collect()
        )
    );
    assert_eq!(
        tag_helper("[A,[],B,{}]"),
        NbtTag::List(NbtList::from_iter(vec![
            wrap(NbtTag::String("A".to_string())),
            wrap(NbtTag::List(NbtList::new())),
            wrap(NbtTag::String("B".to_string())),
            wrap(NbtTag::Compound(NbtCompound::new()))
        ]))
    );
    assert_eq!(
        tag_helper("[\"A\", \"B\", C, \"D\", E]"),
        NbtTag::List(
            "ABCDE"
                .chars()
                .map(|c| c.to_string())
                .map(NbtTag::String)
                .collect()
        )
    );
}

#[test]
fn nbt_uuids() {
    assert_parse!(
        "uuid(\"7c3be0c5-6abd-4fa5-b3f1-0634daa981df\")",
        NbtTag::IntArray(vec![2084298949, 1790791589, -1276049868, -626425377])
    )
}

#[test]
fn nbt_compound() {
    assert_eq!(
        NbtCompound::from_str("{components:[]}").unwrap(),
        nbt!("", {
            "components": []
        })
        .into()
    )
}

#[test]
fn nbt_numbers() {
    assert_parse!("1e7f", NbtTag::Float(1e7));
    assert_parse!("1e7", NbtTag::Double(1e7));
    assert_parse!("10", NbtTag::Int(10));
    assert_parse!("-25", NbtTag::Int(-25));
    assert_parse!("5b", NbtTag::Byte(5));
    assert_parse!("-128b", NbtTag::Byte(-128));
    assert_parse!("127b", NbtTag::Byte(127));
    assert_parse!("50l", NbtTag::Long(50));
    assert_parse!("50L", NbtTag::Long(50));

    assert_parse!("255UB", NbtTag::Byte(-1));
    assert_parse!("10SB", NbtTag::Byte(10));

    // assert_parse!("0x1f", NbtTag::Int(0x1f));

    const TRUE: NbtTag = NbtTag::Byte(1);
    const FALSE: NbtTag = NbtTag::Byte(0);
    assert_parse!("bool(500)", TRUE);
    assert_parse!("bool(0)", FALSE);
    assert_parse!("bool(-0.9999999F)", FALSE);
    assert_parse!("bool(-0.9999999D)", FALSE);
    assert_parse!("bool(true)", TRUE);
    assert_parse!("bool(false)", FALSE);
}

#[test]
fn nbt_fails() {
    assert_matches!(r#"{"": {}}"#.parse::<NbtTag>(), Err(_));

    assert_matches!("1a".parse::<NbtTag>(), Err(_));
    // assert_matches!("0x".parse::<NbtTag>(), Err(_));
    assert_matches!("_1E1".parse::<NbtTag>(), Err(_));
    assert_matches!("1_E1".parse::<NbtTag>(), Err(_));
    assert_matches!("1E_1".parse::<NbtTag>(), Err(_));
    assert_matches!("1E1_".parse::<NbtTag>(), Err(_));
    // assert_matches!("1E.1".parse::<NbtTag>(), Err(_));
    // assert_matches!("1E1.".parse::<NbtTag>(), Err(_));
}

#[test]
fn big_data_parser() {
    let input = include_str!("data/bigdata.snbt");
    let nbt = NbtCompound::from_str(input).unwrap();
    // assert_eq!(nbt.to_string(), input);
}

#[test]
fn nbt_strings() {
    assert_parse!(r#""\N{Snowman}""#, NbtTag::String("\u{2603}".to_owned()));
    assert_parse!(r#""\N{sNoWmAn}""#, NbtTag::String("\u{2603}".to_owned()));
    assert_parse!(r#""\N{Low Line}""#, NbtTag::String("_".to_owned()));
    assert_parse!(
        r#""\N{MODIFIER LETTER SMALL TURNED R WITH LONG LEG AND RETROFLEX HOOK}""#,
        NbtTag::String("\u{107A7}".to_owned())
    );

    // Minecraft does not follow UAX44-LM2 (loose matching)
    assert_parse_failure!(r#""\N{Low-Line}""#);
}
