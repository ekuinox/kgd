//! 自動クローズ・毎時同期の判定のテスト。

use std::sync::atomic::{AtomicUsize, Ordering};

use mockall::predicate::eq;

use kgd_domain::ThreadState;

use crate::test_support::{empty_sync_service, entry, fixed_clock, utc};

use super::*;

/// 一日の始まり(08:00)より前(07:00)の場合、最新エントリはまだ同じ日報日なので
/// ボタンを一切送らない(times(0))ことを確認する。
#[tokio::test]
async fn auto_close_skips_before_day_start_hour() {
    let mut repo = MockDiaryRepository::new();
    // 2025-01-01 の日報が最新。2025-01-02 07:00 はまだ 2025-01-01 の日報日
    repo.expect_get_latest_entry()
        .times(1)
        .returning(|| Ok(Some(entry(100, utc(2025, 1, 1, 0, 0)))));
    let mut gateway = MockDiscordGateway::new();
    gateway.expect_send_close_and_new_button().times(0);
    gateway
        .expect_send_write_channel_new_diary_button()
        .times(0);
    let clock = fixed_clock(utc(2025, 1, 2, 7, 0));

    let m = maintenance(repo, gateway, clock, empty_sync_service());
    m.check_auto_close().await.unwrap();
}

/// 最新エントリが前日でスレッドがアクティブ(未アーカイブ)な場合、対象スレッドへ
/// クローズボタンを、書き込み用チャンネルへ新規日報ボタンを送ること、および同日中の
/// 2 回目では再送せず repo への問い合わせ自体が発生しない(get_latest_entry times(1))ことを確認する。
#[tokio::test]
async fn auto_close_sends_button_for_stale_active_thread() {
    let mut repo = MockDiaryRepository::new();
    // 最新エントリは前日のもの
    repo.expect_get_latest_entry()
        .times(1)
        .returning(|| Ok(Some(entry(100, utc(2025, 1, 1, 0, 0)))));
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_thread_state()
        .with(eq(100u64))
        .times(1)
        .returning(|_| {
            Ok(Some(ThreadState {
                is_public_thread: true,
                archived: false,
                locked: false,
            }))
        });
    gateway
        .expect_send_close_and_new_button()
        .with(eq(100u64))
        .times(1)
        .returning(|_| Ok(()));
    gateway
        .expect_send_write_channel_new_diary_button()
        .with(eq(500u64))
        .times(1)
        .returning(|_| Ok(()));
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let m = maintenance(repo, gateway, clock, empty_sync_service());
    m.check_auto_close().await.unwrap();

    // 同じ日のうちは再送しない（repo への 2 回目の問い合わせ自体が発生しない）
    m.check_auto_close().await.unwrap();
}

/// 最新エントリのスレッドがアーカイブ済みの場合、クローズボタンは送らず(times(0))、
/// 書き込み用チャンネルへの新規日報ボタンは送ることを確認する。
#[tokio::test]
async fn auto_close_skips_archived_thread() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_latest_entry()
        .times(1)
        .returning(|| Ok(Some(entry(100, utc(2025, 1, 1, 0, 0)))));
    let mut gateway = MockDiscordGateway::new();
    gateway.expect_thread_state().times(1).returning(|_| {
        Ok(Some(ThreadState {
            is_public_thread: true,
            archived: true,
            locked: false,
        }))
    });
    gateway.expect_send_close_and_new_button().times(0);
    // スレッドがアーカイブ済みでも書き込み用チャンネルへは送信する
    gateway
        .expect_send_write_channel_new_diary_button()
        .with(eq(500u64))
        .times(1)
        .returning(|_| Ok(()));
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let m = maintenance(repo, gateway, clock, empty_sync_service());
    m.check_auto_close().await.unwrap();
}

/// 毎時同期は初回呼び出しでは現在スロットを記録するだけで同期せず、同一スロットでもスキップし、
/// スロットが変わった時に初めて 1 回だけ同期(get_entries_in_date_range times(1))することを確認する。
#[tokio::test]
async fn hourly_sync_records_only_on_first_call_then_syncs_on_slot_change() {
    let mut repo = MockDiaryRepository::new();
    // スロット切り替え後の 1 回だけ同期が走る
    repo.expect_get_entries_in_date_range()
        .times(1)
        .returning(|_, _| Ok(vec![]));
    let gateway = MockDiscordGateway::new();

    // 1 回目: 10 時（記録のみ）、2 回目: 10 時（同一スロット）、3 回目以降: 11 時（同期）
    let call_count = AtomicUsize::new(0);
    let mut clock = MockClock::new();
    clock.expect_now().returning(move || {
        let i = call_count.fetch_add(1, Ordering::SeqCst);
        if i < 2 {
            utc(2025, 1, 1, 10, 0)
        } else {
            utc(2025, 1, 1, 11, 0)
        }
    });

    let m = maintenance(repo, gateway, clock, empty_sync_service());
    m.check_hourly_sync().await.unwrap(); // 記録のみ
    m.check_hourly_sync().await.unwrap(); // 同一スロット: skip
    m.check_hourly_sync().await.unwrap(); // スロット変化: sync
}
