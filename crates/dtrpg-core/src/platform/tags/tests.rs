//! Tests for tags stored as extended attributes.

#![allow(clippy::unwrap_used)]

use super::*;

fn names(tags: &[&str]) -> Vec<String> {
    tags.iter().map(|tag| (*tag).to_owned()).collect()
}

fn macos_names(tags: &[MacosTag]) -> Vec<&str> {
    tags.iter().map(|tag| tag.name.as_str()).collect()
}

#[test]
fn macos_tags_round_trip() {
    let bytes = encode_macos(&names(&["solo", "to run"]), &[]).unwrap();

    let decoded = decode_macos(&bytes).unwrap();

    assert_eq!(macos_names(&decoded), ["solo", "to run"]);
    assert!(decoded.iter().all(|tag| tag.suffix.is_none()));
}

#[test]
fn macos_tags_are_written_as_a_binary_property_list() {
    let bytes = encode_macos(&names(&["solo"]), &[]).unwrap();

    assert!(bytes.starts_with(b"bplist00"));
}

#[test]
fn a_macos_color_suffix_is_read_apart_from_the_name() {
    let existing = encode_macos(&names(&["Red\n6", "solo"]), &[]).unwrap();

    let decoded = decode_macos(&existing).unwrap();

    assert_eq!(decoded[0],
               MacosTag { name:   "Red".to_owned(),
                          suffix: Some("6".to_owned()), });
    assert_eq!(decoded[1],
               MacosTag { name:   "solo".to_owned(),
                          suffix: None, });
}

#[test]
fn rewriting_macos_tags_keeps_the_color_of_a_tag_that_stays() {
    let existing = [MacosTag { name:   "Red".to_owned(),
                               suffix: Some("6".to_owned()), },
                    MacosTag { name:   "old".to_owned(),
                               suffix: Some("2".to_owned()), }];

    // The user's spelling is "red"; the color Finder gave "Red" stays.
    let bytes = encode_macos(&names(&["red", "new"]), &existing).unwrap();
    let decoded = decode_macos(&bytes).unwrap();

    assert_eq!(decoded[0],
               MacosTag { name:   "red".to_owned(),
                          suffix: Some("6".to_owned()), });
    assert_eq!(decoded[1],
               MacosTag { name:   "new".to_owned(),
                          suffix: None, });
}

#[test]
fn macos_bytes_that_are_not_a_list_of_strings_are_invalid() {
    let not_a_plist = decode_macos(b"not a property list").unwrap_err();
    assert_eq!(not_a_plist.kind(), io::ErrorKind::InvalidData);

    let mut dictionary = Vec::new();
    Value::Dictionary(plist::Dictionary::new()).to_writer_binary(&mut dictionary)
                                               .unwrap();
    assert_eq!(decode_macos(&dictionary).unwrap_err().kind(),
               io::ErrorKind::InvalidData);

    let mut numbers = Vec::new();
    Value::Array(vec![Value::Integer(1.into())]).to_writer_binary(&mut numbers)
                                                .unwrap();
    assert_eq!(decode_macos(&numbers).unwrap_err().kind(),
               io::ErrorKind::InvalidData);
}

#[test]
fn linux_tags_are_a_comma_separated_list() {
    assert_eq!(encode_linux(&names(&["solo", "to run"])), b"solo,to run");
    assert_eq!(decode_linux(b"solo,to run").unwrap(),
               names(&["solo", "to run"]));
}

#[test]
fn linux_decoding_ignores_spaces_around_names_and_empty_entries() {
    assert_eq!(decode_linux(b" solo , ,to run,").unwrap(),
               names(&["solo", "to run"]));
    assert!(decode_linux(b"").unwrap().is_empty());
}

#[test]
fn linux_bytes_that_are_not_utf8_are_invalid() {
    assert_eq!(decode_linux(&[0xFF, 0xFE]).unwrap_err().kind(),
               io::ErrorKind::InvalidData);
}

/// The attribute the running platform keeps tags in.
fn tag_attribute() -> &'static str {
    if cfg!(target_os = "macos") {
        MACOS_ATTRIBUTE
    }
    else {
        LINUX_ATTRIBUTE
    }
}

/// A file in a temporary directory, which must support extended attributes.
fn file_in(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let path = dir.path().join("book.pdf");
    std::fs::write(&path, b"data").unwrap();
    path
}

#[test]
fn a_file_with_no_tag_attribute_has_no_tags() {
    let dir = tempfile::tempdir().unwrap();

    assert!(read(&file_in(&dir)).unwrap().is_empty());
}

#[test]
fn tags_written_to_a_file_are_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = file_in(&dir);

    write(&path, &names(&["solo", "to run"])).unwrap();

    assert_eq!(read(&path).unwrap(), names(&["solo", "to run"]));
}

#[test]
fn writing_tags_replaces_the_ones_the_file_had() {
    let dir = tempfile::tempdir().unwrap();
    let path = file_in(&dir);
    write(&path, &names(&["solo", "to run"])).unwrap();

    write(&path, &names(&["co-op"])).unwrap();

    assert_eq!(read(&path).unwrap(), names(&["co-op"]));
}

#[test]
fn writing_no_tags_removes_the_attribute() {
    let dir = tempfile::tempdir().unwrap();
    let path = file_in(&dir);
    write(&path, &names(&["solo"])).unwrap();

    write(&path, &[]).unwrap();

    assert!(read(&path).unwrap().is_empty());
    // Not merely an empty list: the attribute itself is gone.
    assert!(xattr::get(&path, tag_attribute()).unwrap().is_none());
}

#[test]
fn writing_no_tags_to_a_file_without_any_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();

    write(&file_in(&dir), &[]).unwrap();
}

#[test]
fn tags_on_a_missing_file_are_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.pdf");

    assert!(read(&missing).is_err());
    assert!(write(&missing, &names(&["solo"])).is_err());
}
