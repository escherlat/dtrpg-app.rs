//! Tests for deciding which side's tags win.

use super::*;

fn tags(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// The decision for a file with these tags, an index with these tags, and a
/// snapshot of these (`None` for never synced), with no times.
fn untimed(snapshot: Option<&[&str]>, file: &[&str], index: &[&str]) -> Decision {
    timed(snapshot, file, index, None, None)
}

fn timed(snapshot: Option<&[&str]>, file: &[&str], index: &[&str], file_changed_at: Option<i64>,
         index_changed_at: Option<i64>)
         -> Decision {
    let snapshot = snapshot.map(tags);
    decide(Sides { snapshot: snapshot.as_deref(),
                   file: &tags(file),
                   index: &tags(index),
                   file_changed_at,
                   index_changed_at })
}

#[test]
fn nothing_changed_since_they_agreed() {
    assert_eq!(untimed(Some(&["solo"]), &["solo"], &["solo"]),
               Decision::InSync);
}

#[test]
fn spelling_and_order_do_not_count_as_a_difference() {
    assert_eq!(untimed(Some(&["solo", "to run"]),
                       &["To Run", "SOLO"],
                       &["solo", "to run"]),
               Decision::InSync);
}

#[test]
fn a_file_and_an_index_with_no_tags_that_never_synced_need_nothing() {
    assert_eq!(untimed(None, &[], &[]), Decision::InSync);
}

#[test]
fn matching_tags_that_were_never_recorded_are_recorded() {
    assert_eq!(untimed(None, &["solo"], &["solo"]), Decision::Agreed);
    assert_eq!(untimed(Some(&["old"]), &["solo"], &["solo"]),
               Decision::Agreed);
}

#[test]
fn a_change_in_the_index_alone_is_written_to_the_file() {
    assert_eq!(untimed(Some(&["solo"]), &["solo"], &["solo", "to run"]),
               Decision::WriteFile);
}

#[test]
fn the_last_tag_removed_in_the_index_clears_the_file() {
    assert_eq!(untimed(Some(&["solo"]), &["solo"], &[]),
               Decision::WriteFile);
}

#[test]
fn a_change_in_the_file_alone_is_read_into_the_index() {
    assert_eq!(untimed(Some(&["solo"]), &["solo", "stray"], &["solo"]),
               Decision::ReadFile);
}

#[test]
fn a_tag_removed_in_the_file_is_read_as_a_removal() {
    assert_eq!(untimed(Some(&["solo", "to run"]), &["solo"], &["solo", "to run"]),
               Decision::ReadFile);
}

#[test]
fn every_tag_removed_in_the_file_is_read_as_a_removal() {
    assert_eq!(untimed(Some(&["solo"]), &[], &["solo"]), Decision::ReadFile);
}

#[test]
fn tags_on_a_file_before_it_was_ever_synced_are_read_into_an_index_with_none() {
    assert_eq!(untimed(None, &["from finder"], &[]), Decision::ReadFile);
}

#[test]
fn tags_in_the_index_before_a_file_was_ever_synced_are_written_to_a_file_with_none() {
    assert_eq!(untimed(None, &[], &["solo"]), Decision::WriteFile);
}

#[test]
fn when_both_changed_the_later_change_wins() {
    let both = (Some(&["solo"][..]), &["file"][..], &["index"][..]);

    assert_eq!(timed(both.0, both.1, both.2, Some(200), Some(100)),
               Decision::ReadFile);
    assert_eq!(timed(both.0, both.1, both.2, Some(100), Some(200)),
               Decision::WriteFile);
}

#[test]
fn when_both_changed_at_the_same_time_the_index_wins() {
    assert_eq!(timed(Some(&["solo"]), &["file"], &["index"], Some(100), Some(100)),
               Decision::WriteFile);
}

#[test]
fn when_both_changed_and_a_time_is_missing_the_index_wins() {
    let args = (Some(&["solo"][..]), &["file"][..], &["index"][..]);

    assert_eq!(timed(args.0, args.1, args.2, None, Some(100)),
               Decision::WriteFile);
    assert_eq!(timed(args.0, args.1, args.2, Some(200), None),
               Decision::WriteFile);
    assert_eq!(untimed(args.0, args.1, args.2), Decision::WriteFile);
}

#[test]
fn a_first_sync_where_both_have_different_tags_is_a_conflict_too() {
    assert_eq!(timed(None, &["file"], &["index"], Some(200), Some(100)),
               Decision::ReadFile);
    assert_eq!(timed(None, &["file"], &["index"], None, None),
               Decision::WriteFile);
}

#[test]
fn times_are_ignored_when_only_one_side_changed() {
    assert_eq!(timed(Some(&["solo"]), &["solo"], &["new"], Some(900), Some(1)),
               Decision::WriteFile);
    assert_eq!(timed(Some(&["solo"]), &["new"], &["solo"], Some(1), Some(900)),
               Decision::ReadFile);
}
