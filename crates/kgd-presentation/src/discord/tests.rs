//! discord モジュールの単体テスト。

use super::*;

#[test]
fn is_authorized_allows_everyone_when_admins_empty() {
    assert!(is_authorized(&[], 1234));
}

#[test]
fn is_authorized_allows_listed_user() {
    assert!(is_authorized(&[1, 2, 3], 2));
}

#[test]
fn is_authorized_rejects_unlisted_user() {
    assert!(!is_authorized(&[1, 2, 3], 99));
}
