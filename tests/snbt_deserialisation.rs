use std::str::FromStr as _;

use bytes::Bytes;
use crab_nbt::{nbt, NbtCompound, NbtList, NbtTag};

macro_rules! assert_parse {
    ($input:expr, $expected:expr) => {
        assert_eq!($input.parse(), Ok($expected))
    };
}

fn tag_helper(s: &str) -> NbtTag {
    NbtTag::from_str(s).unwrap()
}

#[test]
fn nbt_tag() {
    assert_eq!(tag_helper("Hello"), NbtTag::String("Hello".to_owned()));
    assert_parse!(r#""true""#, NbtTag::String("true".to_string()));
    assert_parse!(r#""false""#, NbtTag::String("false".to_string()));
}

#[test]
fn nbt_list() {
    assert_eq!(tag_helper("[]"), NbtTag::List(NbtList::new()));
    assert!("[,]".parse::<NbtTag>().is_err());
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
        tag_helper("[A,[],B,{},]"),
        NbtTag::List(NbtList::from_iter(vec![
            NbtTag::String("A".to_string()),
            NbtTag::List(NbtList::new()),
            NbtTag::String("B".to_string()),
            NbtTag::Compound(NbtCompound::new()),
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
    );
    assert_parse!(
        "uuid(\"7c3be0c5-6abd-4fa5-b3f1-634daa981df\")",
        NbtTag::IntArray(vec![2084298949, 1790791589, -1276049868, -626425377])
    );
    assert_parse!(
        "uuid(\"7-c3be0c5-6abd4fa5b3f10634-daa981d-f\")",
        NbtTag::IntArray(vec![7, -523958732, -1742929920, 15])
    );
    assert_parse!(
        "uuid(\"7-c-3-be0c56abd4fa5-b3f10634daa981df\")",
        NbtTag::IntArray(vec![7, 786435, 1336215092, -626425377])
    );

    assert!("uuid(7c3be0c5-6abd-4fa5-b3f1-0634daa981df)"
        .parse::<NbtTag>()
        .is_err());
    assert!("uuid(\"7c3be0c56abd4fa5b3f10634daa981df\")"
        .parse::<NbtTag>()
        .is_err());
    assert!("uuid(\"7c3be0c5-6abd-4fa5-b3f1-0634-daa981df\")"
        .parse::<NbtTag>()
        .is_err());
    assert!("uuid(\"-7c3be0c56abd-4fa5b3f1-0634daa9-81df\")"
        .parse::<NbtTag>()
        .is_err());
    assert!("uuid(\"7--c3be0c56-abd4fa5b-3f10634d-aa981df\")"
        .parse::<NbtTag>()
        .is_err());
}

#[test]
fn nbt_compound() {
    assert_eq!(
        NbtCompound::from_str("{components:[]}").unwrap(),
        nbt!("", {
            "components": []
        })
        .into()
    );

    assert_parse!("{}", NbtTag::Compound(NbtCompound::new()));
    assert!("{,}".parse::<NbtTag>().is_err());
    assert_parse!(
        "{\"a\":1,}",
        NbtTag::Compound(NbtCompound::from_iter([("a".into(), 1.into())]))
    );
}

#[test]
fn nbt_numbers() {
    assert_parse!("1e7f", NbtTag::Float(1e7));
    assert_parse!("1e7", NbtTag::Double(1e7));
    assert_parse!("0.0_4E10f", NbtTag::Float(0.0_4E10));
    assert_parse!("10", NbtTag::Int(10));
    assert_parse!("-25", NbtTag::Int(-25));
    assert_parse!("5b", NbtTag::Byte(5));
    assert_parse!("-128b", NbtTag::Byte(-128));
    assert_parse!("127b", NbtTag::Byte(127));
    assert_parse!("50l", NbtTag::Long(50));
    assert_parse!("50L", NbtTag::Long(50));

    assert_parse!("255UB", NbtTag::Byte(-1));
    assert_parse!("10SB", NbtTag::Byte(10));
    assert_parse!("0b0b", NbtTag::Byte(0));

    assert_parse!("0x1f", NbtTag::Int(0x1f));

    const TRUE: NbtTag = NbtTag::Byte(1);
    const FALSE: NbtTag = NbtTag::Byte(0);
    assert_parse!("bool(500)", TRUE);
    assert_parse!("bool(0)", FALSE);
    assert_parse!("bool(-0.9999999F)", FALSE);
    assert_parse!("bool(-0.9999999D)", FALSE);
    assert_parse!("bool(TRuE)", TRUE);
    assert_parse!("bool(faLSe)", FALSE);
}

#[test]
fn nbt_arrays() {
    assert_parse!(
        "[I; 2084298949, 1790791589, -1276049868, -626425377]",
        NbtTag::IntArray(vec![2084298949, 1790791589, -1276049868, -626425377])
    );
    assert_parse!("[B;]", NbtTag::ByteArray(Bytes::new()));
    assert_parse!("[I;]", NbtTag::IntArray(Vec::new()));
    assert!("[I;,]".parse::<NbtTag>().is_err());
    assert_parse!("[I; 0, 1B, 2S, 3I]", NbtTag::IntArray(vec![0, 1, 2, 3]));
    assert_parse!(
        "[L; 0, 1B, 2S, 3I, 4L,]",
        NbtTag::LongArray(vec![0, 1, 2, 3, 4])
    );
    assert_parse!(
        "[B; 0, 0b1b, 2, 0x3]",
        NbtTag::ByteArray(vec![0, 1, 2, 3].into())
    );
}

#[test]
fn nbt_fails() {
    assert!(r#"{"": {}}"#.parse::<NbtTag>().is_err());

    assert!("1a".parse::<NbtTag>().is_err());
    assert!("0x".parse::<NbtTag>().is_err());
    assert!("_1E1".parse::<NbtTag>().is_err());
    assert!("._1E1".parse::<NbtTag>().is_err());
    assert!("_.1E1".parse::<NbtTag>().is_err());
    assert!("1_E1".parse::<NbtTag>().is_err());
    assert!("1E_1".parse::<NbtTag>().is_err());
    assert!("1E1_".parse::<NbtTag>().is_err());
    assert!("1E.1".parse::<NbtTag>().is_err());
    assert!("1E1.".parse::<NbtTag>().is_err());
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
    assert!(NbtTag::from_str(r#""\N{Low-Line}""#).is_err());
}
