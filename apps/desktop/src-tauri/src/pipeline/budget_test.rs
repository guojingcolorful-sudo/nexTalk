//! Behaviour tests for the latency measurement core (02-01 Task 1).
//!
//! Every instant in this file comes from a scripted clock — the rig takes the
//! crate's one injectable clock trait ([`crate::sim::source::TimeSource`]) and
//! these tests hand it values. Nothing here sleeps, and nothing touches the
//! network: the rig's vendor arithmetic is exercised with the *whole-request*
//! numbers the vendor experiments measured, never with a live socket.

use std::cell::Cell;
use std::collections::BTreeMap;

use super::budget::{
    assert_within_budget, BudgetVerdict, LatencyMark, Stage, StageAlert, Waterfall,
    WaterfallAggregator, WaterfallError, WaterfallRecorder, E2E_BUDGET_MS, MAX_TRACKED_SEGMENTS,
    STT_FIRST_PARTIAL_BUDGET_MS, TRANSLATE_TTFB_BUDGET_MS, TTS_FIRST_AUDIO_BUDGET_MS,
};
use crate::sim::source::TimeSource;

/// Clock double: hands out the scripted instants in order, then keeps repeating
/// the last one (mirrors `sim/source_test.rs` — the crate has exactly one
/// injectable clock trait, and this is not a second one).
struct ScriptedClock {
    values: Vec<u64>,
    cursor: Cell<usize>,
}

impl ScriptedClock {
    fn new(values: Vec<u64>) -> Self {
        assert!(!values.is_empty(), "a scripted clock needs values");
        Self {
            values,
            cursor: Cell::new(0),
        }
    }
}

impl TimeSource for ScriptedClock {
    fn elapsed_ms(&self) -> u64 {
        let index = self.cursor.get();
        self.cursor.set((index + 1).min(self.values.len() - 1));
        self.values[index]
    }
}

/// Five boundary marks in waterfall order, one per [`Stage::ALL`] entry.
fn marks_at(segment_id: u64, cold: bool, at_ms: [u64; 5]) -> Vec<LatencyMark> {
    Stage::ALL
        .iter()
        .copied()
        .zip(at_ms)
        .map(|(stage, at_ms)| LatencyMark {
            stage,
            segment_id,
            at_ms,
            cold,
        })
        .collect()
}

/// Per-stage service durations (the whole-request numbers a vendor reports).
fn durations(ms: [u64; 5]) -> BTreeMap<Stage, u64> {
    Stage::ALL.iter().copied().zip(ms).collect()
}

fn waterfall(segment_id: u64, cold: bool, at_ms: [u64; 5]) -> Waterfall {
    Waterfall::from_marks(&marks_at(segment_id, cold, at_ms)).expect("complete, ordered marks")
}

// Test 1 -------------------------------------------------------------------

#[test]
fn five_boundaries_yield_adjacent_gaps_and_the_e2e_stopwatch() {
    let mut recorder = WaterfallRecorder::new(ScriptedClock::new(vec![0, 120, 300, 520, 560]), 1, false);
    for stage in Stage::ALL {
        recorder.mark(stage);
    }
    let w = recorder.finish().expect("five marks, strictly ordered");

    assert_eq!(w.stage_ms[&Stage::MicCallback], 0);
    assert_eq!(w.stage_ms[&Stage::SttFirstPartial], 120);
    assert_eq!(w.stage_ms[&Stage::TranslateFirstToken], 180);
    assert_eq!(w.stage_ms[&Stage::TtsFirstAudio], 220);
    assert_eq!(w.stage_ms[&Stage::PlaybackFirstSample], 40);
    assert_eq!(w.e2e_ms, 560);
    assert_eq!(w.serial_sum_ms, 560);
    assert_eq!(w.overlap_ms, 0);
    assert_eq!(w.verdict, BudgetVerdict::WithinBudget);
    assert!(assert_within_budget(&w).is_ok());
}

#[test]
fn missing_duplicate_and_out_of_order_marks_are_hard_errors() {
    // 缺边界：只注入了四个边界（缺 输出首帧）。
    let mut four = marks_at(1, false, [0, 100, 200, 300, 400]);
    four.pop();
    assert_eq!(
        Waterfall::from_marks(&four).unwrap_err(),
        WaterfallError::MissingStage {
            stage: Stage::PlaybackFirstSample
        }
    );

    // 乱序：阶段顺序正确，但时间戳回退。
    assert!(matches!(
        Waterfall::from_marks(&marks_at(1, false, [0, 100, 90, 300, 400])).unwrap_err(),
        WaterfallError::OutOfOrder {
            stage: Stage::TranslateFirstToken,
            ..
        }
    ));

    // 重复：同一阶段注入两次会静默覆盖，必须显式报错。
    let mut duplicated = marks_at(1, false, [0, 100, 200, 300, 400]);
    duplicated.push(LatencyMark {
        stage: Stage::SttFirstPartial,
        segment_id: 1,
        at_ms: 150,
        cold: false,
    });
    assert_eq!(
        Waterfall::from_marks(&duplicated).unwrap_err(),
        WaterfallError::DuplicateStage {
            stage: Stage::SttFirstPartial
        }
    );

    // 跨片段混合：不同 segment_id 的 marks 不得拼成一条瀑布。
    assert!(matches!(
        Waterfall::from_marks(&marks_at(1, false, [0, 100, 200, 300, 400])
            .into_iter()
            .map(|mark| if mark.stage == Stage::TtsFirstAudio {
                LatencyMark { segment_id: 2, ..mark }
            } else {
                mark
            })
            .collect::<Vec<_>>())
        .unwrap_err(),
        WaterfallError::SegmentMismatch { .. }
    ));

    assert_eq!(Waterfall::from_marks(&[]).unwrap_err(), WaterfallError::Empty);
}

// Test 2 -------------------------------------------------------------------

#[test]
fn overlap_is_proven_when_the_serial_sum_exceeds_the_stopwatch() {
    // STT is still working on its whole clip (700 ms) while the translation
    // stage has already started (223 ms) and TTS is mid-stream (1300 ms):
    // the naive sum is 2243 ms, the listener's stopwatch reads 950 ms.
    let w = Waterfall::from_marks_with_durations(
        &marks_at(1, false, [0, 300, 480, 900, 950]),
        &durations([0, 700, 223, 1300, 20]),
    )
    .expect("complete marks and durations");

    assert_eq!(w.e2e_ms, 950);
    assert_eq!(w.serial_sum_ms, 2243);
    assert_eq!(w.overlap_ms, 1293);
    assert!(w.overlap_ms > 0, "overlapping stages must be provable");
    assert_eq!(w.verdict, BudgetVerdict::WithinBudget);
    assert!(assert_within_budget(&w).is_ok());
}

// Test 3 -------------------------------------------------------------------

#[test]
fn vendor_whole_request_arithmetic_passes_once_the_stages_overlap() {
    // 研究修正 2：讯飞 0.7s + 翻译 0.223s + 火山 1.3s 朴素相加 2.22s 超预算，
    // 但那是三个非并发的整请求耗时，不是流式边界耗时。
    let w = Waterfall::from_marks_with_durations(
        &marks_at(1, false, [0, 180, 320, 600, 650]),
        &durations([0, 700, 223, 1300, 0]),
    )
    .expect("complete marks and durations");

    assert_eq!(w.serial_sum_ms, 2223);
    assert!(w.serial_sum_ms > E2E_BUDGET_MS, "朴素串行和必须超预算");
    assert!(w.e2e_ms <= E2E_BUDGET_MS, "重叠后的实测 e2e 必须在预算内");
    assert!(w.overlap_ms >= 223, "重叠量至少是被并掉的翻译段");
    assert_eq!(w.verdict, BudgetVerdict::WithinBudget);
    assert!(assert_within_budget(&w).is_ok());
}

// Test 4 -------------------------------------------------------------------

#[test]
fn one_millisecond_over_budget_fails_and_attributes_the_stage() {
    // 边界间隙：STT 700 / 翻译 300 / TTS 900 / 输出 101 → e2e = 2001ms。
    let w = waterfall(7, true, [0, 700, 1000, 1900, 2001]);

    assert_eq!(w.e2e_ms, 2001);
    assert_eq!(
        w.verdict,
        BudgetVerdict::OverBudget {
            stage: Stage::TtsFirstAudio,
            over_by_ms: 1
        },
        "超支必须归因到耗时最大的阶段"
    );

    let breach = assert_within_budget(&w).expect_err("预算门禁必须硬失败，不得仅告警");
    let text = breach.to_string();
    assert!(text.contains("2001"), "超支文案要写明 e2e：{text}");
    assert!(text.contains("+1ms"), "超支文案要写明超支毫秒数：{text}");
    assert!(
        text.contains(Stage::TtsFirstAudio.label_zh()),
        "超支文案要写明归因阶段：{text}"
    );
    assert_eq!(breach.segment_id, 7);
}

// Test 5 -------------------------------------------------------------------

#[test]
fn cold_and_warm_segments_are_judged_and_reported_separately() {
    let cold = waterfall(1, true, [0, 900, 1200, 2100, 2500]);
    let warm = waterfall(2, false, [0, 200, 500, 800, 900]);
    assert!(matches!(cold.verdict, BudgetVerdict::OverBudget { .. }));
    assert_eq!(warm.verdict, BudgetVerdict::WithinBudget);

    let mut aggregator = WaterfallAggregator::new();
    aggregator.push(cold);
    aggregator.push(warm);
    let report = aggregator.report();

    // 两套数字各自保留，绝不跨冷热求平均。
    assert_eq!(report.cold.segments, 1);
    assert_eq!(report.cold.e2e.p50_ms, 2500);
    assert_eq!(report.cold.e2e.p95_ms, 2500);
    assert_eq!(report.cold.over_budget_segments, 1);

    assert_eq!(report.warm.segments, 1);
    assert_eq!(report.warm.e2e.p50_ms, 900);
    assert_eq!(report.warm.e2e.p95_ms, 900);
    assert_eq!(report.warm.over_budget_segments, 0);
}

// Test 6 -------------------------------------------------------------------

#[test]
fn session_aggregation_reports_percentiles_and_drops_the_oldest_beyond_the_cap() {
    let mut aggregator = WaterfallAggregator::new();
    for index in 0..600u64 {
        // 固定阶段间隙 + 递增的播放段：e2e = 1000 + index。
        aggregator.push(waterfall(index + 1, false, [0, 300, 500, 800, 1000 + index]));
    }

    let report = aggregator.report();
    assert_eq!(report.cold.segments, 0);
    assert_eq!(
        report.warm.segments, MAX_TRACKED_SEGMENTS,
        "超过上限的片段必须被丢弃，聚合器不得无界增长"
    );

    // 最近秩（nearest-rank）：保留 i = 88..600 → e2e 1088..1599。
    assert_eq!(report.warm.e2e.p50_ms, 1343);
    assert_eq!(report.warm.e2e.p95_ms, 1574);
    assert_eq!(report.warm.stages[&Stage::SttFirstPartial].p50_ms, 300);
    assert_eq!(report.warm.stages[&Stage::SttFirstPartial].p95_ms, 300);
    // 播放段 = 200 + i → 288..799。
    assert_eq!(report.warm.stages[&Stage::PlaybackFirstSample].p50_ms, 543);
    assert_eq!(report.warm.stages[&Stage::PlaybackFirstSample].p95_ms, 774);
    assert_eq!(report.warm.over_budget_segments, 0);

    // 空类也是合法报告（面板据 segments 判空态），不是 panic。
    assert_eq!(report.cold.e2e.p50_ms, 0);
    assert!(report.cold.latest.is_none());
    assert!(report.warm.latest.is_some());
}

// Test 7 -------------------------------------------------------------------

#[test]
fn stage_budgets_alert_individually_even_when_the_segment_stays_within_budget() {
    assert_eq!(STT_FIRST_PARTIAL_BUDGET_MS, 500);
    assert_eq!(TRANSLATE_TTFB_BUDGET_MS, 500);
    assert_eq!(TTS_FIRST_AUDIO_BUDGET_MS, 800);

    let stt = waterfall(1, false, [0, 501, 600, 700, 800]);
    assert_eq!(stt.verdict, BudgetVerdict::WithinBudget);
    assert_eq!(
        stt.stage_alerts(),
        vec![StageAlert {
            stage: Stage::SttFirstPartial,
            over_by_ms: 1
        }]
    );

    let translate = waterfall(2, false, [0, 100, 601, 700, 800]);
    assert_eq!(
        translate.stage_alerts(),
        vec![StageAlert {
            stage: Stage::TranslateFirstToken,
            over_by_ms: 1
        }]
    );

    let tts = waterfall(3, false, [0, 100, 200, 1001, 1100]);
    assert_eq!(
        tts.stage_alerts(),
        vec![StageAlert {
            stage: Stage::TtsFirstAudio,
            over_by_ms: 1
        }]
    );

    let clean = waterfall(4, false, [0, 100, 200, 300, 400]);
    assert!(clean.stage_alerts().is_empty());
}
