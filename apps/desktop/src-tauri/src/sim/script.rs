//! The four-round simulated interview script (01-05) — pure data.
//!
//! Each round is a self-contained micro-scene: the interviewer asks (listening),
//! a strategy card lands while the question is read, the user's Chinese answer
//! streams in together with the English line the cloned voice will speak, and
//! the generating phase renders until the round ends. `source.rs` derives
//! `Vec<ServerEvent>` from these offsets, so script content stays deterministic
//! and unit-testable without a clock.
//!
//! Round 1 is LOCKED verbatim against the reference mockup (01-02). Rounds 2-4
//! are D-03 planner discretion: the technical-question arc that keeps the demo
//! believable (线上故障排查 → 分布式设计 → 职业规划).

/// Functional colour of a strategy card in the UI-SPEC palette (三功能色:
/// green = user/pronunciation, yellow = AI strategy, blue = translation /
/// system). The locked wire union carries no tone — renderers derive it from
/// the event type — so this is design metadata for the script table itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Yellow: a strategy card the copilot produced.
    AiStrategy,
}

/// Per-round wall-clock offsets, in milliseconds relative to the round's start.
///
/// Phase mapping: listening = `[0, generating_at_ms)`, generating =
/// `[generating_at_ms, end_at_ms)`. Offsets are strictly ordered (strategy <
/// answer < generating < end).
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// The strategy card lands while the interviewer's question is read.
    pub strategy_at_ms: u64,
    /// The user's answer starts streaming.
    pub answer_at_ms: u64,
    /// The cloned-voice render indicator turns on.
    pub generating_at_ms: u64,
    /// The next round's listening phase starts here.
    pub end_at_ms: u64,
}

/// Strategy card content (UI-SPEC yellow card).
#[derive(Debug, Clone, Copy)]
pub struct Strategy {
    pub title: &'static str,
    pub bullets: &'static [&'static str],
    pub tone: Tone,
}

/// One interview round: what is said, what the copilot suggests, and when.
#[derive(Debug, Clone, Copy)]
pub struct Round {
    /// Stable round id; also the strategy event's `roundId` ("r1".."r4").
    pub id: &'static str,
    /// 模拟数据 badge value carried by every round (D-03: the demo is never
    /// presented as a real session).
    pub tag: &'static str,
    /// The interviewer's question, as heard (English) and as subtitled (中文).
    pub interviewer_en: &'static str,
    pub interviewer_zh: &'static str,
    /// What the user says in Chinese.
    pub user_zh: &'static str,
    /// What the cloned voice says — the English line of the user's bubble.
    pub user_en: &'static str,
    pub strategy: Strategy,
    pub timing: Timing,
}

/// Badge every simulated round carries (D-03).
pub const TAG_MOCK: &str = "模拟数据";

pub const ROUNDS: [Round; 4] = [
    // ---------------------------------------------------------------- r1 ---
    // LOCKED: byte-exact against the reference mockup. Do not paraphrase.
    Round {
        id: "r1",
        tag: TAG_MOCK,
        interviewer_en: "Could you walk me through the specific steps you took to optimize the database?",
        interviewer_zh: "你能详细说一下你优化数据库的具体步骤吗？",
        user_zh: "首先，我们分析了慢查询日志，发现主要瓶颈在商品详情页的连表查询上。",
        user_en: "First we analysed the slow query log and found the bottleneck was a multi-table join on the product detail page.",
        strategy: Strategy {
            title: "数据库优化",
            bullets: &["慢查询日志定位", "拆连表查询", "Redis 缓存层"],
            tone: Tone::AiStrategy,
        },
        timing: Timing {
            strategy_at_ms: 2_500,
            answer_at_ms: 6_000,
            generating_at_ms: 6_500,
            end_at_ms: 8_500,
        },
    },
    // ---------------------------------------------------------------- r2 ---
    // D-03 discretion: technical questions.
    Round {
        id: "r2",
        tag: TAG_MOCK,
        interviewer_en: "What would you do when a production service degrades at 2 AM?",
        interviewer_zh: "如果凌晨两点线上服务出现性能退化，你会怎么处理？",
        user_zh: "我会先确认影响面，再看监控和日志，判断是流量突增还是依赖抖动。",
        user_en: "I would first scope the impact, then check the dashboards and logs to tell whether it is a traffic spike or a flaky dependency.",
        strategy: Strategy {
            title: "故障排查",
            bullets: &["先定影响面", "看监控与日志", "回滚或限流"],
            tone: Tone::AiStrategy,
        },
        timing: Timing {
            strategy_at_ms: 2_200,
            answer_at_ms: 5_200,
            generating_at_ms: 5_600,
            end_at_ms: 7_000,
        },
    },
    // ---------------------------------------------------------------- r3 ---
    // D-03 discretion: technical questions.
    Round {
        id: "r3",
        tag: TAG_MOCK,
        interviewer_en: "Describe a distributed system you designed and its failure mode.",
        interviewer_zh: "请描述一个你设计过的分布式系统，以及它的失效模式。",
        user_zh: "我设计过一个订单分片系统，主从切换时会有短暂的写入不可用。",
        user_en: "I designed a sharded order system; its failure mode was a short write outage during primary failover.",
        strategy: Strategy {
            title: "分布式设计",
            bullets: &["分片键选择", "副本与一致性", "失效模式演练"],
            tone: Tone::AiStrategy,
        },
        timing: Timing {
            strategy_at_ms: 2_600,
            answer_at_ms: 6_400,
            generating_at_ms: 6_900,
            end_at_ms: 8_600,
        },
    },
    // ---------------------------------------------------------------- r4 ---
    // D-03 discretion: the closing round (career plan + the reverse question).
    Round {
        id: "r4",
        tag: TAG_MOCK,
        interviewer_en: "Where do you see yourself in two years, and what do you expect from the team?",
        interviewer_zh: "你未来两年有什么规划？你希望团队给你什么样的支持？",
        user_zh: "我希望两年内能独立负责一个核心系统，也希望团队有明确的反馈机制。",
        user_en: "In two years I want to own a core system independently, and I expect regular, direct feedback from the team.",
        strategy: Strategy {
            title: "职业规划",
            bullets: &["两年成长目标", "对团队的期待", "反问面试官"],
            tone: Tone::AiStrategy,
        },
        timing: Timing {
            strategy_at_ms: 2_400,
            answer_at_ms: 5_800,
            generating_at_ms: 6_200,
            end_at_ms: 7_800,
        },
    },
];

/// Event ids, stable across transports (Tauri `session` emit + WS broadcast).
/// The engine appends a `-r{n}` suffix to a replayed event so 重听 never
/// collides with the original (the desktop keys bubbles by id, the phone
/// dedupes strategy cards by id).
pub fn question_id(round_index: usize) -> String {
    format!("{}-q", ROUNDS[round_index].id)
}

pub fn answer_id(round_index: usize) -> String {
    format!("{}-a", ROUNDS[round_index].id)
}

pub fn strategy_id(round_index: usize) -> String {
    format!("s-{}", ROUNDS[round_index].id)
}

/// Total scripted length: the demo interview runs ~32 seconds.
pub fn total_duration_ms() -> u64 {
    ROUNDS.iter().map(|round| round.timing.end_at_ms).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_duration_is_the_sum_of_every_round() {
        assert_eq!(total_duration_ms(), 8_500 + 7_000 + 8_600 + 7_800);
    }

    #[test]
    fn ids_are_stable_and_round_scoped() {
        assert_eq!(question_id(0), "r1-q");
        assert_eq!(answer_id(0), "r1-a");
        assert_eq!(strategy_id(0), "s-r1");
        assert_eq!(question_id(3), "r4-q");
    }
}
