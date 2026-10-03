//! Latency rig (02-01 T1.2): the end-to-end stopwatch, the ≤2000 ms hard gate,
//! and the optional live variant.
//!
//! The rig is the gate every downstream wave is judged by, so the default run
//! is **deterministic and zero-network**: a scripted stage double injects the
//! five streaming boundary marks and the budget assertion must hold. The live
//! variant (`latency_e2e_cold`) is `#[ignore]`d because it needs vendor
//! credentials; it drives the *real* stages once 02-02/02-03 land them, and
//! until then it must fail loudly rather than pass silently.
//!
//! 研究修正 2 is asserted here, not assumed: 讯飞 0.7s + 翻译 0.223s + 火山 1.3s
//! are *whole-request* numbers, their naive sum (2.22s) exceeds the budget, and
//! the cascade only passes because the stages genuinely overlap.

use nextalk_desktop_lib::pipeline::budget::{
    assert_within_budget, LatencyMark, Stage, Waterfall, WaterfallAggregator, E2E_BUDGET_MS,
};

/// Vendor credentials the live variant needs — the names the experiment
/// framework already documents in `tools/vendor-experiments/.env.example`
/// (D-04: keys live in the environment, never in the repo).
const LIVE_ENV_KEYS: [&str; 7] = [
    "XFYUN_APP_ID",
    "XFYUN_API_KEY",
    "XFYUN_API_SECRET",
    "DEEPGRAM_API_KEY",
    "DEEPSEEK_API_KEY",
    "VOLC_TTS_APP_ID",
    "VOLC_TTS_ACCESS_TOKEN",
];

// --------------------------------------------------------------- stage double ---

/// Scripted stage double — the stand-in clock-injectable source for the real
/// stages (02-02 replaces it behind the same boundary marks). One instance
/// records one segment: the five boundaries with their scripted offsets.
#[derive(Debug, Default)]
struct ScriptedStages;

impl ScriptedStages {
    /// Records the five boundaries at the scripted offsets (ms since the
    /// segment's mic callback) and returns the segment's waterfall.
    fn record(&self, segment_id: u64, cold: bool, offsets: [u64; 5]) -> Waterfall {
        let marks: Vec<LatencyMark> = Stage::ALL
            .iter()
            .copied()
            .zip(offsets)
            .map(|(stage, at_ms)| LatencyMark {
                stage,
                segment_id,
                at_ms,
                cold,
            })
            .collect();
        Waterfall::from_marks(&marks).expect("the script injects five ordered boundaries")
    }
}

/// Prints one waterfall in the shape a human reads during a live session
/// (`-- --nocapture`): per stage ms + share of the stopwatch, then the verdict.
fn print_waterfall(w: &Waterfall) {
    println!(
        "片段 {}{} —— e2e {}ms（朴素串行和 {}ms，重叠 {}ms）",
        w.segment_id,
        if w.cold { "（冷启动）" } else { "（热路径）" },
        w.e2e_ms,
        w.serial_sum_ms,
        w.overlap_ms
    );
    for stage in Stage::ALL {
        let ms = w.stage_ms.get(&stage).copied().unwrap_or(0);
        let share = if w.e2e_ms == 0 {
            0
        } else {
            (ms * 100).div_ceil(w.e2e_ms)
        };
        println!("  {:<28} {:>5}ms  {:>3}%", stage.label_zh(), ms, share);
    }
    match w.verdict {
        nextalk_desktop_lib::pipeline::budget::BudgetVerdict::WithinBudget => {
            println!("  判定：在预算内（≤ {E2E_BUDGET_MS}ms）");
        }
        nextalk_desktop_lib::pipeline::budget::BudgetVerdict::OverBudget { stage, over_by_ms } => {
            println!("  判定：超支 +{over_by_ms}ms（归因 {}）", stage.label_zh());
        }
    }
}

// ------------------------------------------------------------------ the rig ---

#[test]
fn five_scripted_segments_pass_the_budget_gate() {
    let stages = ScriptedStages;
    let mut aggregator = WaterfallAggregator::new();
    let scripts: [[u64; 5]; 5] = [
        [0, 260, 480, 900, 1180],  // cold start (first segment of the session)
        [0, 240, 470, 880, 1210],
        [0, 250, 500, 910, 1250],
        [0, 230, 520, 940, 1300],
        [0, 255, 505, 925, 1225],
    ];

    for (index, offsets) in scripts.iter().enumerate() {
        let cold = index == 0;
        let w = stages.record(index as u64 + 1, cold, *offsets);
        print_waterfall(&w);
        assert_within_budget(&w).unwrap_or_else(|breach| panic!("{breach}"));
        aggregator.push(w);
    }

    let report = aggregator.report();
    println!(
        "会话聚合：冷启动 {} 片段 / 热路径 {} 片段",
        report.cold.segments, report.warm.segments
    );
    println!(
        "e2e：冷 p50 {}ms / p95 {}ms；热 p50 {}ms / p95 {}ms",
        report.cold.e2e.p50_ms,
        report.cold.e2e.p95_ms,
        report.warm.e2e.p50_ms,
        report.warm.e2e.p95_ms
    );

    assert_eq!(report.cold.segments, 1);
    assert_eq!(report.warm.segments, 4);
    assert!(report.cold.e2e.p95_ms <= E2E_BUDGET_MS);
    assert!(report.warm.e2e.p95_ms <= E2E_BUDGET_MS);
    assert_eq!(report.cold.over_budget_segments + report.warm.over_budget_segments, 0);
}

#[test]
fn vendor_whole_request_sum_over_budget_passes_once_the_stages_overlap() {
    // 讯飞 0.7s + 翻译 0.223s + 火山 1.3s 朴素相加 = 2223ms > 2000ms 预算；
    // 但三段的整请求耗时彼此重叠，流式边界实测 e2e 仍在预算内（研究修正 2）。
    let marks: Vec<LatencyMark> = Stage::ALL
        .iter()
        .copied()
        .zip([0u64, 180, 320, 600, 650])
        .map(|(stage, at_ms)| LatencyMark {
            stage,
            segment_id: 1,
            at_ms,
            cold: true,
        })
        .collect();
    let durations = [
        (Stage::MicCallback, 0u64),
        (Stage::SttFirstPartial, 700),
        (Stage::TranslateFirstToken, 223),
        (Stage::TtsFirstAudio, 1_300),
        (Stage::PlaybackFirstSample, 0),
    ]
    .into_iter()
    .collect();

    let w = Waterfall::from_marks_with_durations(&marks, &durations).expect("complete inputs");
    print_waterfall(&w);

    assert_eq!(w.serial_sum_ms, 2_223);
    assert!(w.serial_sum_ms > E2E_BUDGET_MS);
    assert!(w.e2e_ms <= E2E_BUDGET_MS);
    assert!(w.overlap_ms >= 223);
    assert_within_budget(&w).expect("重叠让该片段留在预算内");
}

// -------------------------------------------------------------- live variant ---

/// Credentials the live variant is missing, given a lookup over the process
/// environment (injected so the gate is testable without touching the real
/// environment).
fn missing_credentials(configured: impl Fn(&str) -> bool) -> Vec<&'static str> {
    LIVE_ENV_KEYS
        .iter()
        .copied()
        .filter(|key| !configured(key))
        .collect()
}

/// Precondition of the live measurement: either every credential is present
/// (and the body may proceed to the real stages), or the variant fails with an
/// explicit reason — never a silent pass.
fn live_precondition(configured: impl Fn(&str) -> bool) -> Result<(), String> {
    let missing = missing_credentials(configured);
    if !missing.is_empty() {
        return Err(format!(
            "live 延迟测量缺少供应商凭据：{}（见 tools/vendor-experiments/.env.example）——未配置时不得静默通过",
            missing.join("、")
        ));
    }
    Err("真实级联阶段尚未接线（02-02/02-03）——live 变体在接线前不得通过".to_string())
}

#[test]
fn live_variant_never_passes_silently() {
    let without_keys = live_precondition(|_| false).expect_err("no credentials configured");
    for key in LIVE_ENV_KEYS {
        assert!(
            without_keys.contains(key),
            "缺少凭据的报错必须点名 {key}：{without_keys}"
        );
    }

    let with_keys = live_precondition(|_| true).expect_err("the real stages do not exist yet");
    assert!(with_keys.contains("02-02"), "{with_keys}");
}

/// Live end-to-end cold measurement (`cargo test --test latency_rig -- --ignored`).
///
/// Ignored by default: it needs vendor credentials, and it drives the real
/// cascade — which 02-02/02-03 land. Its job today is to prove the rig refuses
/// to report a number it did not measure: without keys it exits non-zero with
/// the missing names, and with keys it exits non-zero saying the stages are not
/// wired yet.
#[test]
#[ignore = "live measurement — needs vendor credentials and the real stages (02-02/02-03)"]
fn latency_e2e_cold() {
    if let Err(message) =
        live_precondition(|key| std::env::var(key).map(|value| !value.is_empty()).unwrap_or(false))
    {
        panic!("{message}");
    }

    // 02-02/02-03 wire the real stages here: drive one cold segment through the
    // cascade with `WaterfallRecorder` over `RealClock`, then assert the
    // waterfall and print it. Until then this test must never pass.
    unimplemented!("02-02/02-03 接线后由真实阶段驱动本变体");
}
