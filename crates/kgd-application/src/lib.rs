//! kgd のアプリケーション層。ユースケースとポート (IO 抽象) を提供する。
//!
//! このクレートは serenity / sqlx / reqwest などの IO ライブラリに依存してはならない。
//! 外部 IO はすべて [`ports`] の trait 経由で扱い、テストでは mockall のモックを使う。

mod check_server_status;
mod manage_diary_lifecycle;
pub mod ports;
mod relay_write_channel_message;
mod run_diary_maintenance;
mod scheduler;
mod sync_diary_message;
#[cfg(test)]
mod test_support;
mod wake_server;

pub use check_server_status::CheckServerStatus;
pub use manage_diary_lifecycle::{
    CloseAndNewPrecheck, DiaryCreateOutcome, DiaryLifecycleSettings, ManageDiaryLifecycle,
};
pub use relay_write_channel_message::{
    RelaySettings, RelayWriteChannelMessage, WriteChannelEvent, run_relay_worker,
};
pub use run_diary_maintenance::{DiaryMaintenanceSettings, RunDiaryMaintenance};
pub use scheduler::{AutoCloseJob, HourlySyncJob, ScheduledJob};
pub use sync_diary_message::SyncDiaryMessage;
pub use wake_server::{WakeOutcome, WakeServer};
