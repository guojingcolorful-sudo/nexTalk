//! Latency measurement rig — the ≤2000 ms budget gate (AUDI-04).
//!
//! # The five boundaries are streaming first-byte instants
//!
//! [`Stage`] names five **streaming TTFB boundaries**, in waterfall order:
//! mic callback → STT first partial → translation first token → TTS first audio
//! frame → first output PCM consumed. They answer "when did the listener first
//! hear something", not "how long did a request take".
//!
//! The vendor-experiment numbers (讯飞 0.7s / 翻译 0.223s / 火山 1.3s) are
//! **whole-request** latencies — a full clip to its final result, a full
//! sentence to `SESSION_FINISHED`. They must not be fed into the waterfall as
//! if they were boundary gaps. They belong in
//! [`Waterfall::from_marks_with_durations`] as *stage service durations*, where
//! their naive sum is exactly the number the overlap proof is measured against.
//!
//! # Overlap is proven, not assumed
//!
//! Three stages that run one after another would cost 700 + 223 + 1300 = 2223 ms
//! and blow the budget. The cascade only passes because the stages genuinely
//! overlap: 2.22s of *work* can happen inside ≤2s of *wall time*, but only if
//! the rig can show it. [`Waterfall::serial_sum_ms`] is the naive serial sum and
//! [`Waterfall::overlap_ms`] = `serial_sum_ms - e2e_ms`; `overlap_ms > 0` is the
//! proof, and the live rig prints it.
//!
//! # Interface contract for the stage implementations (02-02)
//!
//! Every stage implementation calls [`WaterfallRecorder::mark`] **exactly once**
//! per segment, at its own first streamed byte/frame (first partial, first
//! token, first audio frame, first PCM consumed). The `cold` flag is supplied by
//! the session-start path — the first segment of a session is cold — and is
//! never guessed from elapsed time, so cold and warm numbers can never be
//! silently averaged into one budget claim (T-02-02).
//!
//! # Determinism
//!
//! All timing goes through [`crate::sim::source::TimeSource`] — the crate has
//! exactly one injectable clock trait, and this module does not invent a second
//! one. Production passes `RealClock`; tests pass a scripted double.
//!
//! # Privacy (T-02-01)
//!
//! A mark and a waterfall hold only a stage, a segment id, milliseconds and the
//! cold flag. No URL, no headers, no provider payload — vendor URLs carry
//! `authorization=` query parameters, and none of that may travel with timing
//! data.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use serde::Serialize;

use crate::sim::source::TimeSource;

/// 端到端预算：mic 回调 → 输出首个 PCM 被消费，含冷启动握手成本（AUDI-04）。
pub const E2E_BUDGET_MS: u64 = 2_000;

/// STT 首 partial 阶段预算 ≤500ms（02-AI-SPEC §4b 延迟预算）。
pub const STT_FIRST_PARTIAL_BUDGET_MS: u64 = 500;

/// 翻译 TTFT 阶段预算 ≤500ms（02-AI-SPEC §4b 延迟预算）。
pub const TRANSLATE_TTFB_BUDGET_MS: u64 = 500;

/// TTS 首音频阶段预算 ≤800ms（02-AI-SPEC §4b 延迟预算）。
pub const TTS_FIRST_AUDIO_BUDGET_MS: u64 = 800;

/// 会话级聚合保留的片段上限（T-02-03：有界环，长会话不得无界增长）。
pub const MAX_TRACKED_SEGMENTS: usize = 512;

/// The five streaming boundaries, in waterfall order.
///
/// Declaration order **is** the boundary order (`Ord` is derived), so the
/// `BTreeMap<Stage, _>` in [`Waterfall`] iterates as the waterfall reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    /// Mic callback delivered the segment's first PCM frame (the stopwatch origin).
    MicCallback,
    /// The user-path STT returned its first partial for this segment.
    SttFirstPartial,
    /// The translator returned its first token.
    TranslateFirstToken,
    /// The cloned-voice TTS returned its first audio frame.
    TtsFirstAudio,
    /// The output device consumed the first PCM frame (the stopwatch end).
    PlaybackFirstSample,
}

impl Stage {
    /// The waterfall order.
    pub const ALL: [Stage; 5] = [
        Stage::MicCallback,
        Stage::SttFirstPartial,
        Stage::TranslateFirstToken,
        Stage::TtsFirstAudio,
        Stage::PlaybackFirstSample,
    ];

    /// 中文阶段名（面板与超支文案共用，中文锁定）。
    pub fn label_zh(self) -> &'static str {
        match self {
            Stage::MicCallback => "麦克风回调",
            Stage::SttFirstPartial => "STT 首 partial",
            Stage::TranslateFirstToken => "翻译首 token",
            Stage::TtsFirstAudio => "TTS 首音频",
            Stage::PlaybackFirstSample => "输出首帧",
        }
    }

    /// 该阶段的预算（02-AI-SPEC §4b）；无阶段预算的边界返回 `None`，由 e2e 覆盖。
    pub fn budget_ms(self) -> Option<u64> {
        match self {
            Stage::SttFirstPartial => Some(STT_FIRST_PARTIAL_BUDGET_MS),
            Stage::TranslateFirstToken => Some(TRANSLATE_TTFB_BUDGET_MS),
            Stage::TtsFirstAudio => Some(TTS_FIRST_AUDIO_BUDGET_MS),
            Stage::MicCallback | Stage::PlaybackFirstSample => None,
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label_zh())
    }
}

/// One boundary instant for one segment.
///
/// `at_ms` is milliseconds since the session started — the same origin as
/// [`TimeSource::elapsed_ms`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyMark {
    pub stage: Stage,
    pub segment_id: u64,
    pub at_ms: u64,
    /// Supplied by the session-start path: the session's first segment is cold.
    /// Never derived from elapsed time (T-02-02).
    pub cold: bool,
}

/// 预算判定。超支即失败——没有「仅告警」的开关（AUDI-04 的门禁语义）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BudgetVerdict {
    WithinBudget,
    OverBudget {
        /// 归因：耗时最大的阶段。
        stage: Stage,
        #[serde(rename = "overByMs")]
        over_by_ms: u64,
    },
}

/// 阶段级告警（02-AI-SPEC §4b 单阶段预算）。片段整体在预算内时也可能触发。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageAlert {
    pub stage: Stage,
    #[serde(rename = "overByMs")]
    pub over_by_ms: u64,
}

/// 一个片段的延迟瀑布。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Waterfall {
    pub segment_id: u64,
    pub cold: bool,
    /// 每阶段耗时（ms）。边界推导时 = 与上一相邻边界的差值；重叠场景由阶段自报的
    /// 服务时长覆盖（见 [`Waterfall::from_marks_with_durations`]）。
    pub stage_ms: BTreeMap<Stage, u64>,
    /// 停表：`MicCallback` → `PlaybackFirstSample`。
    pub e2e_ms: u64,
    /// 朴素串行和 = Σ 阶段时长。
    pub serial_sum_ms: u64,
    /// `serial_sum_ms - e2e_ms`（钳到 0）：> 0 即证明阶段真实重叠。
    pub overlap_ms: u64,
    pub verdict: BudgetVerdict,
}

impl Waterfall {
    /// Builds a waterfall from the five boundary marks alone, treating the
    /// stages as consecutive: each stage's duration is the gap to the previous
    /// boundary (`MicCallback` is the origin and has duration 0).
    ///
    /// This is the honest reading when the caller has no per-stage service
    /// measurement — and it makes `serial_sum_ms == e2e_ms`, i.e. it claims no
    /// overlap. Use [`Waterfall::from_marks_with_durations`] when the stages
    /// report how long they actually worked.
    pub fn from_marks(marks: &[LatencyMark]) -> Result<Self, WaterfallError> {
        let boundaries = validate_marks(marks)?;
        let durations = boundary_gaps(&boundaries);
        Self::from_marks_with_durations(marks, &durations)
    }

    /// Builds a waterfall from the boundary marks **plus** each stage's own
    /// service duration.
    ///
    /// `durations` is where whole-request numbers belong: `SttFirstPartial`
    /// 700 ms means the STT stage worked 700 ms on that segment, regardless of
    /// when its first partial landed. The sum of those durations is
    /// [`Waterfall::serial_sum_ms`]; comparing it against the stopwatch is what
    /// proves the cascade overlaps.
    ///
    /// Fails loudly on incomplete input — no silent zero-fill.
    pub fn from_marks_with_durations(
        marks: &[LatencyMark],
        durations: &BTreeMap<Stage, u64>,
    ) -> Result<Self, WaterfallError> {
        let boundaries = validate_marks(marks)?;
        for stage in Stage::ALL {
            if !durations.contains_key(&stage) {
                return Err(WaterfallError::MissingDuration { stage });
            }
        }

        let e2e_ms = boundaries[&Stage::PlaybackFirstSample] - boundaries[&Stage::MicCallback];
        let serial_sum_ms: u64 = Stage::ALL
            .iter()
            .map(|stage| durations.get(stage).copied().unwrap_or(0))
            .sum();
        let overlap_ms = serial_sum_ms.saturating_sub(e2e_ms);
        let verdict = if e2e_ms > E2E_BUDGET_MS {
            BudgetVerdict::OverBudget {
                stage: heaviest_stage(durations),
                over_by_ms: e2e_ms - E2E_BUDGET_MS,
            }
        } else {
            BudgetVerdict::WithinBudget
        };

        Ok(Self {
            segment_id: boundaries_segment_id(marks),
            cold: marks[0].cold,
            stage_ms: durations.clone(),
            e2e_ms,
            serial_sum_ms,
            overlap_ms,
            verdict,
        })
    }

    /// 超预算的阶段级告警，按瀑布顺序返回（02-AI-SPEC §4b）。
    pub fn stage_alerts(&self) -> Vec<StageAlert> {
        let mut alerts = Vec::new();
        for stage in Stage::ALL {
            let (Some(budget), Some(ms)) = (stage.budget_ms(), self.stage_ms.get(&stage)) else {
                continue;
            };
            if *ms > budget {
                alerts.push(StageAlert {
                    stage,
                    over_by_ms: ms - budget,
                });
            }
        }
        alerts
    }
}

/// 预算门禁：超支返回 [`BudgetBreach`]，调用方必须失败（`Err`，不是警告）。
pub fn assert_within_budget(waterfall: &Waterfall) -> Result<(), BudgetBreach> {
    match waterfall.verdict {
        BudgetVerdict::WithinBudget => Ok(()),
        BudgetVerdict::OverBudget { stage, over_by_ms } => Err(BudgetBreach {
            segment_id: waterfall.segment_id,
            e2e_ms: waterfall.e2e_ms,
            stage,
            over_by_ms,
        }),
    }
}

/// 超支明细：文案含 e2e、超支毫秒数与归因阶段（中文锁定，与面板同源）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetBreach {
    pub segment_id: u64,
    pub e2e_ms: u64,
    pub stage: Stage,
    pub over_by_ms: u64,
}

impl fmt::Display for BudgetBreach {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "片段 {} 端到端 {}ms 超出 {}ms 预算 +{}ms（归因：{}）",
            self.segment_id,
            self.e2e_ms,
            E2E_BUDGET_MS,
            self.over_by_ms,
            self.stage.label_zh()
        )
    }
}

impl std::error::Error for BudgetBreach {}

/// 瀑布构造失败：缺边界、乱序、重复或跨片段混合。绝不静默补零。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaterfallError {
    /// 一个边界都没有。
    Empty,
    /// 缺某个边界（注入点漏打时间戳）。
    MissingStage { stage: Stage },
    /// 同一阶段注入了两次（会静默覆盖，必须报错）。
    DuplicateStage { stage: Stage },
    /// 时间戳回退：后一个边界早于前一个边界。
    OutOfOrder {
        stage: Stage,
        at_ms: u64,
        previous_ms: u64,
    },
    /// 不同片段的 marks 被拼进同一条瀑布。
    SegmentMismatch { expected: u64, found: u64 },
    /// 同一片段的 marks 冷热标记不一致。
    ColdFlagMismatch { segment_id: u64 },
    /// 重叠场景下缺少某阶段的服务时长。
    MissingDuration { stage: Stage },
}

impl fmt::Display for WaterfallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WaterfallError::Empty => f.write_str("waterfall needs at least one latency mark"),
            WaterfallError::MissingStage { stage } => {
                write!(f, "missing latency mark for stage {stage:?}")
            }
            WaterfallError::DuplicateStage { stage } => {
                write!(f, "duplicate latency mark for stage {stage:?}")
            }
            WaterfallError::OutOfOrder {
                stage,
                at_ms,
                previous_ms,
            } => write!(
                f,
                "latency marks out of order: {stage:?} at {at_ms}ms is before {previous_ms}ms"
            ),
            WaterfallError::SegmentMismatch { expected, found } => write!(
                f,
                "latency marks span two segments (expected {expected}, found {found})"
            ),
            WaterfallError::ColdFlagMismatch { segment_id } => {
                write!(f, "segment {segment_id} mixes cold and warm marks")
            }
            WaterfallError::MissingDuration { stage } => {
                write!(f, "missing service duration for stage {stage:?}")
            }
        }
    }
}

impl std::error::Error for WaterfallError {}

/// 会话级报告的一个冷热类（冷启动 / 热路径各自独立，绝不跨类求平均）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassReport {
    /// 该类保留的片段数（受 [`MAX_TRACKED_SEGMENTS`] 限制）。
    pub segments: usize,
    pub e2e: Percentiles,
    /// 每阶段的下分位。空类为 0（面板据 `segments` 判空态）。
    pub stages: BTreeMap<Stage, Percentiles>,
    pub over_budget_segments: usize,
    /// 该类最近一个片段的瀑布（面板画的就是它）；无片段时为 `None`。
    pub latest: Option<Waterfall>,
}

/// 会话级瀑布报告：冷启动与热路径两套数字并列，互不覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaterfallReport {
    pub cold: ClassReport,
    pub warm: ClassReport,
}

/// 最近秩（nearest-rank）分位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Percentiles {
    pub p50_ms: u64,
    pub p95_ms: u64,
}

/// 会话级聚合器：冷/热各一条有界环（T-02-03）。
#[derive(Debug, Default, Clone)]
pub struct WaterfallAggregator {
    cold: VecDeque<Waterfall>,
    warm: VecDeque<Waterfall>,
}

impl WaterfallAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一个片段的瀑布；队列满时丢弃最旧的片段，容量恒定有界。
    pub fn push(&mut self, waterfall: Waterfall) {
        let ring = if waterfall.cold {
            &mut self.cold
        } else {
            &mut self.warm
        };
        if ring.len() == MAX_TRACKED_SEGMENTS {
            ring.pop_front();
        }
        ring.push_back(waterfall);
    }

    /// 保留的片段总数（两类之和）。
    pub fn len(&self) -> usize {
        self.cold.len() + self.warm.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 冷启动与热路径各自的 p50/p95 报告。
    pub fn report(&self) -> WaterfallReport {
        WaterfallReport {
            cold: class_report(&self.cold),
            warm: class_report(&self.warm),
        }
    }
}

/// 记录一个片段的五个边界，计时全部来自可注入时钟。
///
/// 生产侧传 `RealClock`；测试传脚本时钟。`segment_id` 与 `cold` 在构造时绑定
/// ——一个 recorder 只服务一个片段，冷热由会话启动路径决定。
#[derive(Debug)]
pub struct WaterfallRecorder<C: TimeSource> {
    clock: C,
    segment_id: u64,
    cold: bool,
    marks: Vec<LatencyMark>,
}

impl<C: TimeSource> WaterfallRecorder<C> {
    pub fn new(clock: C, segment_id: u64, cold: bool) -> Self {
        Self {
            clock,
            segment_id,
            cold,
            marks: Vec::with_capacity(Stage::ALL.len()),
        }
    }

    /// 每个阶段实现在自己的首字节边界调用一次（见模块文档的接口约定）。
    pub fn mark(&mut self, stage: Stage) {
        self.marks.push(LatencyMark {
            stage,
            segment_id: self.segment_id,
            at_ms: self.clock.elapsed_ms(),
            cold: self.cold,
        });
    }

    /// 收尾：缺边界、乱序或重复都会得到明确的 [`WaterfallError`]。
    pub fn finish(self) -> Result<Waterfall, WaterfallError> {
        Waterfall::from_marks(&self.marks)
    }
}

// ------------------------------------------------------------------ internals ---

/// Validates the marks and returns `stage -> boundary instant`.
fn validate_marks(marks: &[LatencyMark]) -> Result<BTreeMap<Stage, u64>, WaterfallError> {
    let Some(first) = marks.first() else {
        return Err(WaterfallError::Empty);
    };
    let segment_id = first.segment_id;
    let cold = first.cold;
    let mut boundaries: BTreeMap<Stage, u64> = BTreeMap::new();

    for mark in marks {
        if mark.segment_id != segment_id {
            return Err(WaterfallError::SegmentMismatch {
                expected: segment_id,
                found: mark.segment_id,
            });
        }
        if mark.cold != cold {
            return Err(WaterfallError::ColdFlagMismatch { segment_id });
        }
        if boundaries.insert(mark.stage, mark.at_ms).is_some() {
            return Err(WaterfallError::DuplicateStage { stage: mark.stage });
        }
    }

    for stage in Stage::ALL {
        if !boundaries.contains_key(&stage) {
            return Err(WaterfallError::MissingStage { stage });
        }
    }

    for pair in Stage::ALL.windows(2) {
        let (previous, next) = (pair[0], pair[1]);
        let (from, to) = (boundaries[&previous], boundaries[&next]);
        if to < from {
            return Err(WaterfallError::OutOfOrder {
                stage: next,
                at_ms: to,
                previous_ms: from,
            });
        }
    }

    Ok(boundaries)
}

fn boundaries_segment_id(marks: &[LatencyMark]) -> u64 {
    marks.first().map(|mark| mark.segment_id).unwrap_or(0)
}

/// Serial reading: each stage owns the gap to the previous boundary.
fn boundary_gaps(boundaries: &BTreeMap<Stage, u64>) -> BTreeMap<Stage, u64> {
    let mut gaps = BTreeMap::new();
    let mut previous: Option<u64> = None;
    for stage in Stage::ALL {
        let at_ms = boundaries[&stage];
        gaps.insert(stage, previous.map_or(0, |prev| at_ms - prev));
        previous = Some(at_ms);
    }
    gaps
}

/// 归因：耗时最大的阶段（并列时取瀑布顺序里更早的那个，保证确定性）。
fn heaviest_stage(stage_ms: &BTreeMap<Stage, u64>) -> Stage {
    let mut heaviest = Stage::MicCallback;
    let mut heaviest_ms = 0;
    for stage in Stage::ALL {
        let ms = stage_ms.get(&stage).copied().unwrap_or(0);
        if ms > heaviest_ms {
            heaviest = stage;
            heaviest_ms = ms;
        }
    }
    heaviest
}

fn class_report(ring: &VecDeque<Waterfall>) -> ClassReport {
    let mut stages = BTreeMap::new();
    for stage in Stage::ALL {
        stages.insert(
            stage,
            percentiles(
                ring.iter()
                    .map(|w| w.stage_ms.get(&stage).copied().unwrap_or(0)),
            ),
        );
    }
    ClassReport {
        segments: ring.len(),
        e2e: percentiles(ring.iter().map(|w| w.e2e_ms)),
        stages,
        over_budget_segments: ring
            .iter()
            .filter(|w| matches!(w.verdict, BudgetVerdict::OverBudget { .. }))
            .count(),
        latest: ring.back().cloned(),
    }
}

fn percentiles(values: impl Iterator<Item = u64>) -> Percentiles {
    let mut sorted: Vec<u64> = values.collect();
    sorted.sort_unstable();
    Percentiles {
        p50_ms: nearest_rank(&sorted, 50),
        p95_ms: nearest_rank(&sorted, 95),
    }
}

/// 最近秩分位：`rank = ceil(p/100 × n)`，取第 rank 小的值（1-based）；空集为 0。
fn nearest_rank(sorted: &[u64], percentile: u64) -> u64 {
    let Some(last) = sorted.last() else {
        return 0;
    };
    let count = sorted.len() as u64;
    let rank = (percentile * count).div_ceil(100).clamp(1, count);
    if rank == count {
        return *last;
    }
    sorted[(rank - 1) as usize]
}
