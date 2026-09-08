//! Round-1 simulation script content (01-02 walking skeleton).
//!
//! Pure data: the exact subtitle/strategy strings that play on a simulated
//! session plus the wall-clock offsets (ms after session start) at which each
//! event fires. The evaluator in `source.rs` derives `Vec<ServerEvent>` from
//! these offsets, so script content is deterministic and unit-testable.

/// Bilingual question from the interviewer (round 1).
pub const QUESTION_EN: &str =
    "Could you walk me through the specific steps you took to optimize the database?";
pub const QUESTION_ZH: &str = "你能详细说一下你优化数据库的具体步骤吗？";

/// User's Chinese-only answer (round 1). `en` stays absent so the H5 renders
/// the "translating" state before the cloud translation arrives (01-05).
pub const ANSWER_ZH: &str = "首先，我们分析了慢查询日志，发现主要瓶颈在商品详情页的连表查询上。";

/// Strategy card content (round 1).
pub const STRATEGY_TITLE: &str = "数据库优化";
pub const STRATEGY_BULLETS: [&str; 3] = ["慢查询日志定位", "拆连表查询", "Redis 缓存层"];

pub const ROUND_ID: &str = "r1";

/// Event ids, stable across transports (Tauri `session` emit + WS broadcast).
pub const QUESTION_ID: &str = "r1-q";
pub const ANSWER_ID: &str = "r1-a";
pub const STRATEGY_ID: &str = "s-r1";

/// Offsets (ms after session start) at which script events fire.
pub const QUESTION_AT_MS: u64 = 0;
pub const STRATEGY_AT_MS: u64 = 2_500;
pub const ANSWER_AT_MS: u64 = 6_000;
pub const GENERATING_AT_MS: u64 = 6_500;
pub const LISTENING_AT_MS: u64 = 8_500;

/// Subtitle sequence numbers, in emission order.
pub const QUESTION_SEQ: u64 = 1;
pub const ANSWER_SEQ: u64 = 2;
