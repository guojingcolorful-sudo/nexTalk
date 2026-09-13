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
/// `[generating_at_ms, end_at_ms)`. Offsets are strictly ordered (question <
/// strategy < answer < generating < end).
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// The interviewer's question opens the round after this lead-in
    /// (UAT-11: the session never starts mid-sentence).
    pub question_at_ms: u64,
    /// The strategy card lands only after the question has been fully read
    /// aloud (UAT-11: hear the question first, then the AI thinks).
    pub strategy_at_ms: u64,
    /// The user's answer starts streaming — after a deliberate thinking gap.
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
    /// The AI's bilingual suggested answer (UAT-8): a complete 中/EN answer
    /// to the interviewer's question, not just a prompt outline.
    pub answer_zh: &'static str,
    pub answer_en: &'static str,
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
            answer_zh: "面试官您好，针对商品详情页的慢查询，我主要做了三步。第一步，通过慢查询日志定位到瓶颈是商品、SKU、库存三张表的连表查询，单次响应超过 800 毫秒。第二步，我把连表拆成两次单表查询，在应用层组装，并把基本信息与库存拆成两个接口并行请求。第三步，为热点商品引入 Redis 缓存，设置 30 秒过期并加上穿透保护，P95 延迟从 800 毫秒降到了 60 毫秒。",
            answer_en: "To optimize the product detail page, I took three steps. First, I used the slow query log to locate the bottleneck — a join across the product, SKU and inventory tables taking over 800 ms per request. Second, I split the join into two single-table queries assembled at the application layer, and served basic info and inventory as two parallel requests. Third, I added a Redis cache with a 30-second TTL and penetration protection for hot products, which brought the P95 latency down from 800 ms to 60 ms.",
            tone: Tone::AiStrategy,
        },
        timing: Timing {
            question_at_ms: 1_500,
            strategy_at_ms: 7_000,
            answer_at_ms: 14_000,
            generating_at_ms: 14_500,
            end_at_ms: 17_000,
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
            answer_zh: "我会分四步处理。第一步，先确认影响面和严重级别，按预案通知相关同学。第二步，同时打开监控大盘和错误日志，快速判断是流量突增还是依赖抖动。第三步，如果是流量问题就扩容或限流，如果是发布引起就立即回滚。第四步，恢复之后做无责复盘，补齐告警和演练。整个过程我会持续同步进展。",
            answer_en: "I would handle it in four steps. First, confirm the blast radius and severity, and page the on-call team. Second, check the dashboards and error logs in parallel to determine whether it is a traffic spike or a flaky dependency. Third, scale out or throttle if it is traffic, and roll back immediately if a release caused it. Fourth, run a blameless postmortem and fill the gaps in alerting and drills. I keep the team posted on progress throughout.",
            tone: Tone::AiStrategy,
        },
        timing: Timing {
            question_at_ms: 1_500,
            strategy_at_ms: 7_000,
            answer_at_ms: 14_000,
            generating_at_ms: 14_500,
            end_at_ms: 17_000,
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
            answer_zh: "我设计过一个订单分片系统，按用户 ID 哈希分成 32 个分片。它的失效模式是主从切换时会有三到五秒的写入不可用。我的应对有三层：写入侧用本地缓冲队列，把切换窗口内的订单做延迟提交；读侧在切换期间降级为读副本，牺牲一点一致性；同时用半同步复制加心跳检测，把切换检测时间压到两秒以内。",
            answer_en: "I designed a sharded order system with 32 shards by user-id hash. Its failure mode was a three-to-five-second write outage during primary failover. I built three layers of mitigation: on the write side, a local buffer queue defers orders submitted during the switchover window; on the read side, reads degrade to the replica, trading a little consistency; and semi-synchronous replication with heartbeats brought failover detection down to under two seconds.",
            tone: Tone::AiStrategy,
        },
        timing: Timing {
            question_at_ms: 1_500,
            strategy_at_ms: 7_000,
            answer_at_ms: 14_000,
            generating_at_ms: 14_500,
            end_at_ms: 17_000,
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
            answer_zh: "未来两年，我希望从模块的执行者成长为核心系统的负责人，能够独立完成方案设计、落地和稳定性兜底。我期望团队有明确的目标拆解和直接的反馈机制，比如定期的 1:1 和代码评审文化。另外我想反问一个问题：团队目前在做的系统里，最希望新人在第一年补齐的能力是什么？",
            answer_en: "In two years I want to grow from delivering modules to owning a core system — designing, shipping and keeping it stable. I expect the team to have clear goal decomposition and direct feedback loops, such as regular one-on-ones and a code review culture. May I ask a question in return: what capability does the team most want a new member to build up in their first year?",
            tone: Tone::AiStrategy,
        },
        timing: Timing {
            question_at_ms: 1_500,
            strategy_at_ms: 7_000,
            answer_at_ms: 14_000,
            generating_at_ms: 14_500,
            end_at_ms: 17_000,
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
        assert_eq!(total_duration_ms(), 17_000 * 4);
    }

    #[test]
    fn ids_are_stable_and_round_scoped() {
        assert_eq!(question_id(0), "r1-q");
        assert_eq!(answer_id(0), "r1-a");
        assert_eq!(strategy_id(0), "s-r1");
        assert_eq!(question_id(3), "r4-q");
    }

    #[test]
    fn every_round_carries_a_bilingual_ai_answer() {
        for round in ROUNDS {
            assert!(
                !round.strategy.answer_zh.trim().is_empty(),
                "round {} needs a Chinese AI answer",
                round.id
            );
            assert!(
                !round.strategy.answer_en.trim().is_empty(),
                "round {} needs an English AI answer",
                round.id
            );
            assert!(
                round.strategy.answer_en.contains(' '),
                "round {} answer_en must be a full sentence",
                round.id
            );
        }
    }
}
