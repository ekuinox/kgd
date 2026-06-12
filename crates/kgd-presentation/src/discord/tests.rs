//! discord モジュールの単体テスト。

use super::*;

/// 管理者リストが空のとき、任意のユーザーが許可される (全員許可) ことを確認する。
#[test]
fn is_authorized_allows_everyone_when_admins_empty() {
    assert!(is_authorized(&[], 1234));
}

/// 管理者リストに含まれるユーザー ID が許可されることを確認する。
#[test]
fn is_authorized_allows_listed_user() {
    assert!(is_authorized(&[1, 2, 3], 2));
}

/// 空でない管理者リストに含まれないユーザー ID が拒否されることを確認する。
#[test]
fn is_authorized_rejects_unlisted_user() {
    assert!(!is_authorized(&[1, 2, 3], 99));
}
