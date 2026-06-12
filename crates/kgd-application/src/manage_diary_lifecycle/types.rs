//! 日報ライフサイクル管理の設定・結果型。

use chrono_tz::Tz;

/// ライフサイクル管理の設定。
#[derive(Debug, Clone)]
pub struct DiaryLifecycleSettings {
    /// 日報の日付計算に使用するタイムゾーン
    pub timezone: Tz,
    /// 日報スレッドを作成するフォーラムチャンネル ID
    pub forum_channel_id: u64,
    /// 日報の書き込み用チャンネル ID
    pub write_channel_id: u64,
}

/// 日報作成（/diary new）の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiaryCreateOutcome {
    /// 既存スレッドを再開した
    Reopened {
        /// 再開したスレッド ID
        thread_id: u64,
    },
    /// 既存スレッドがあるが再開できなかった
    ExistsButNotReopened {
        /// 既存スレッド ID
        thread_id: u64,
    },
    /// 新しくスレッドを作成した
    Created {
        /// 作成したスレッド ID
        thread_id: u64,
        /// Notion ページ URL
        page_url: String,
        /// 既存の Notion ページを再利用したかどうか
        reused_page: bool,
    },
}

/// クローズ & 新規作成の事前確認の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseAndNewPrecheck {
    /// 対象スレッドが日報スレッドではない
    NotDiaryThread,
    /// 対象スレッドが今日の最新の日報である
    AlreadyLatest,
    /// 今日の最新の日報が別に存在する
    LatestExists {
        /// 最新の日報スレッド ID
        thread_id: u64,
    },
    /// 新規作成へ進める
    ReadyToCreate,
}

/// 日報クローズ（/diary close）の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiaryCloseOutcome {
    /// 対象スレッドが日報スレッドではない
    NotDiaryThread,
    /// クローズした
    Closed,
}
