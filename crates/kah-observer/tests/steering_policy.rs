use kah_observer::intake::{Presence, presence};

#[test]
fn recording_presence_does_not_depend_on_kind_and_bands_include_boundaries() {
    assert_eq!(presence(0.8, 0.8, 0.2), Presence::Record);
    assert_eq!(presence(0.2, 0.8, 0.2), Presence::Ignore);
    assert_eq!(presence(0.799, 0.8, 0.2), Presence::Review);
    assert_eq!(presence(0.201, 0.8, 0.2), Presence::Review);
    assert_eq!(presence(0.35, 0.8, 0.35), Presence::Ignore);
}
