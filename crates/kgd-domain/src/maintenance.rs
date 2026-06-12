//! 日報の定期メンテナンス（自動クローズ・毎時同期）の判定ロジック。
//!
//! IO を伴わない純粋関数として実装し、時刻や IO の結果は引数で受ける。

use chrono::{DateTime, NaiveDate, Timelike as _, Utc};
use chrono_tz::Tz;

/// 毎時同期の実行済み時間帯を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiaryHourlySyncSlot {
    /// タイムゾーン基準の日付。
    date: NaiveDate,
    /// タイムゾーン基準の時。
    hour: u32,
}

impl DiaryHourlySyncSlot {
    /// 指定された時刻から毎時同期の判定に使う時間帯を作る。
    ///
    /// `now` を引数で受けることで現在時刻に依存しない単体テストが可能になる。
    pub fn from(now: DateTime<Utc>, timezone: &Tz) -> Self {
        let local = now.with_timezone(timezone);
        Self {
            date: local.date_naive(),
            hour: local.hour(),
        }
    }
}

/// 毎時同期スロットの遷移判定の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HourlySyncDecision {
    /// 起動直後などで現在スロットを記録するだけ。同期は行わない。
    RecordOnly,
    /// 同一スロット内なので何もしない。
    Skip,
    /// スロットが切り替わったので同期する。
    Sync,
}

/// 直近スロットと現在スロットから、毎時同期を行うべきか判定する純粋関数。
pub fn decide_hourly_sync(
    last_slot: Option<DiaryHourlySyncSlot>,
    current_slot: DiaryHourlySyncSlot,
) -> HourlySyncDecision {
    match last_slot {
        None => HourlySyncDecision::RecordOnly,
        Some(slot) if slot == current_slot => HourlySyncDecision::Skip,
        Some(_) => HourlySyncDecision::Sync,
    }
}

/// 自動クローズ通知を送信する前段（IO を呼ぶ前）のゲート判定を行う純粋関数。
///
/// 機能無効・指定時刻前・本日通知済みのいずれかなら `false` を返す。
pub fn should_attempt_auto_close(
    now: DateTime<Utc>,
    timezone: &Tz,
    auto_close_enabled: bool,
    auto_close_hour: u32,
    last_notified: Option<NaiveDate>,
) -> bool {
    if !auto_close_enabled {
        return false;
    }
    let local = now.with_timezone(timezone);
    if local.hour() < auto_close_hour {
        return false;
    }
    last_notified.is_none_or(|date| date != local.date_naive())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    /// テスト用に UTC の DateTime を作る。
    fn utc(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
            .unwrap()
    }

    /// 直近スロットが None（初回起動）の場合は RecordOnly を返し、
    /// 同期せず現在スロットの記録のみ行うことを確認する。
    #[test]
    fn decide_hourly_sync_records_only_on_first_run() {
        let slot = DiaryHourlySyncSlot::from(utc(2025, 1, 1, 10, 0), &chrono_tz::UTC);
        assert_eq!(
            decide_hourly_sync(None, slot),
            HourlySyncDecision::RecordOnly
        );
    }

    /// 直近スロットと現在スロットが同一の場合は Skip を返し、
    /// 同一時間帯では同期しないことを確認する。
    #[test]
    fn decide_hourly_sync_skips_same_slot() {
        let slot = DiaryHourlySyncSlot::from(utc(2025, 1, 1, 10, 0), &chrono_tz::UTC);
        assert_eq!(
            decide_hourly_sync(Some(slot), slot),
            HourlySyncDecision::Skip
        );
    }

    /// 時が切り替わって直近スロットと現在スロットが異なる場合は
    /// Sync を返し、同期を行うことを確認する。
    #[test]
    fn decide_hourly_sync_syncs_on_slot_change() {
        let prev = DiaryHourlySyncSlot::from(utc(2025, 1, 1, 10, 0), &chrono_tz::UTC);
        let now = DiaryHourlySyncSlot::from(utc(2025, 1, 1, 11, 0), &chrono_tz::UTC);
        assert_eq!(
            decide_hourly_sync(Some(prev), now),
            HourlySyncDecision::Sync
        );
    }

    /// スロット生成が指定タイムゾーンに従い、UTC 23:00 が
    /// Asia/Tokyo では翌日 08:00 の時・日付になることを確認する。
    #[test]
    fn hourly_slot_respects_timezone() {
        // UTC 23:00 は Asia/Tokyo では翌日 08:00。
        let now = utc(2025, 1, 1, 23, 0);
        let slot = DiaryHourlySyncSlot::from(now, &chrono_tz::Asia::Tokyo);
        assert_eq!(slot.hour, 8);
        assert_eq!(
            slot.date,
            chrono::NaiveDate::from_ymd_opt(2025, 1, 2).unwrap()
        );
    }

    /// 自動クローズ機能が無効な場合は、時刻や未通知でも
    /// false を返すことを確認する。
    #[test]
    fn should_attempt_auto_close_false_when_disabled() {
        assert!(!should_attempt_auto_close(
            utc(2025, 1, 1, 10, 0),
            &chrono_tz::UTC,
            false,
            8,
            None,
        ));
    }

    /// 機能が有効でも現在時（7時）が自動クローズ時刻（8時）より前なら
    /// false を返し、早すぎる通知を防ぐことを確認する。
    #[test]
    fn should_attempt_auto_close_false_before_hour() {
        assert!(!should_attempt_auto_close(
            utc(2025, 1, 1, 7, 0),
            &chrono_tz::UTC,
            true,
            8,
            None,
        ));
    }

    /// 機能が有効で時刻を満たしていても、本日すでに通知済みなら
    /// false を返し、重複通知を防ぐことを確認する。
    #[test]
    fn should_attempt_auto_close_false_when_already_notified_today() {
        let now = utc(2025, 1, 1, 9, 0);
        let today = now.date_naive();
        assert!(!should_attempt_auto_close(
            now,
            &chrono_tz::UTC,
            true,
            8,
            Some(today),
        ));
    }

    /// 機能が有効・時刻到達済み（9時≧8時）・本日未通知の条件が揃った場合に
    /// true を返すことを確認する。
    #[test]
    fn should_attempt_auto_close_true_when_due_and_not_notified() {
        assert!(should_attempt_auto_close(
            utc(2025, 1, 1, 9, 0),
            &chrono_tz::UTC,
            true,
            8,
            None,
        ));
    }
}
