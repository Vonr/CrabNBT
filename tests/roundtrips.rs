use std::{assert_matches, str::FromStr};

#[test]
pub fn adversarial() {
    let bytes = include_bytes!("./data/adversarial.dat").to_vec();
    let nbt = crab_nbt::Nbt::read(&mut bytes.as_slice());
    assert_matches!(nbt, Ok(_));
    let mut nbt = nbt.unwrap().root_tag;
    nbt.child_tags.sort_by_key(|(k, _)| k.clone());

    let str = include_str!("./data/adversarial.snbt").trim_end();
    let nbt2 = crab_nbt::NbtTag::from_str(str);
    assert_matches!(nbt2, Ok(crab_nbt::NbtTag::Compound(_)));
    let crab_nbt::NbtTag::Compound(mut nbt2) = nbt2.unwrap() else {
        unreachable!();
    };
    nbt2.child_tags.sort_by_key(|(k, _)| k.clone());

    assert_eq!(nbt, nbt2);
}
