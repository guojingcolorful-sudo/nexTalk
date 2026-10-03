//! Vendor stage clients (T2.1–T2.5).
//!
//! One module per vendor plus the contract they all satisfy:
//!
//! | module | vendor | role |
//! |--------|--------|------|
//! | [`config`] | — | credentials, endpoints, the routing table |
//! | [`error`] | — | `RetryClass` and the shared failure classification |
//! | [`traits`] | — | the stage contracts and their deterministic doubles |
//! | `xfyun` (T2.2) | 讯飞 `iat` | user's Chinese speech |
//! | `deepgram` (T2.3) | Deepgram `nova-3` | interviewer's English (never re-voiced) |
//! | `deepseek` (T2.4) | DeepSeek `deepseek-chat` | incremental Chinese → English |
//! | `volc_tts` (T2.5) | 火山 `seed-icl-2.0` | cloned-voice synthesis |
//!
//! # Why enum dispatch
//!
//! The cascade holds one value per stage and swaps vendors by configuration.
//! `dyn Trait` cannot carry `async fn` (the methods here are synchronous and
//! hand back a channel, which is the same thing without the `async-trait`
//! boxing), and an enum keeps the value `Sized`, cloneable and matchable. Each
//! wave that lands a vendor adds one variant and one arm — no trait objects, no
//! allocation per hop inside the 2 s budget.

pub mod config;
pub mod deepgram;
pub mod error;
pub mod traits;
pub mod xfyun;

pub use config::{
    DeepgramCredentials, DeepseekCredentials, Endpoint, Endpoints, RoutingConfig, Secret,
    StageRole, Track, VolcCredentials, XfyunCredentials,
};
pub use error::{classify_http_status, classify_xfyun_code, ErrorKind, RetryClass, StageError};
pub use traits::{
    AbstainReason, AudioChunk, ConfidenceSource, GlossaryEntry, InterviewerTrack, MarkHandle,
    ScriptedStt, ScriptedTranslator, ScriptedTts, SpeakerId, SttEvent, SttPartial, SttSource,
    SttStream, SttUpstream, TokenUsage, Translator, TranslatorEvent, TranslatorStream, TtsEvent,
    TtsSink, TtsStream, TtsUsage, VoiceRef, ZhFragment, AUDIO_QUEUE_FRAMES, EVENT_QUEUE_ITEMS,
};
pub use xfyun::XfyunStt;

// ---------------------------------------------------------------------------
// dispatch
// ---------------------------------------------------------------------------

/// The Chinese STT stage, selected by configuration.
#[derive(Debug, Clone)]
pub enum VendorStt {
    /// Deterministic double: tests, offline demos, and the 02-01 rig.
    Scripted(ScriptedStt),
    /// 讯飞 `iat` — the production Chinese line (T2.2).
    Xfyun(XfyunStt),
    // T2.3 adds `Deepgram(DeepgramStt)` for the interviewer track.
}

impl From<ScriptedStt> for VendorStt {
    fn from(source: ScriptedStt) -> Self {
        Self::Scripted(source)
    }
}

impl From<XfyunStt> for VendorStt {
    fn from(source: XfyunStt) -> Self {
        Self::Xfyun(source)
    }
}

impl SttSource for VendorStt {
    fn provider(&self) -> &'static str {
        match self {
            VendorStt::Scripted(source) => source.provider(),
            VendorStt::Xfyun(source) => source.provider(),
        }
    }

    fn model_version(&self) -> String {
        match self {
            VendorStt::Scripted(source) => source.model_version(),
            VendorStt::Xfyun(source) => source.model_version(),
        }
    }

    fn set_marks(&mut self, marks: MarkHandle) {
        match self {
            VendorStt::Scripted(source) => source.set_marks(marks),
            VendorStt::Xfyun(source) => source.set_marks(marks),
        }
    }

    fn start(&mut self, epoch: u64) -> Result<SttStream, StageError> {
        match self {
            VendorStt::Scripted(source) => source.start(epoch),
            VendorStt::Xfyun(source) => source.start(epoch),
        }
    }
}

/// The translation stage, selected by configuration.
#[derive(Debug, Clone)]
pub enum VendorTranslator {
    Scripted(ScriptedTranslator),
    // T2.4 adds `Deepseek(DeepseekTranslator)`.
}

impl From<ScriptedTranslator> for VendorTranslator {
    fn from(translator: ScriptedTranslator) -> Self {
        Self::Scripted(translator)
    }
}

impl Translator for VendorTranslator {
    fn provider(&self) -> &'static str {
        match self {
            VendorTranslator::Scripted(translator) => translator.provider(),
        }
    }

    fn model_version(&self) -> String {
        match self {
            VendorTranslator::Scripted(translator) => translator.model_version(),
        }
    }

    fn set_marks(&mut self, marks: MarkHandle) {
        match self {
            VendorTranslator::Scripted(translator) => translator.set_marks(marks),
        }
    }

    fn translate(
        &mut self,
        fragment: &ZhFragment,
        glossary: &[GlossaryEntry],
        epoch: u64,
    ) -> Result<TranslatorStream, StageError> {
        match self {
            VendorTranslator::Scripted(translator) => {
                translator.translate(fragment, glossary, epoch)
            }
        }
    }
}

/// The synthesis stage, selected by configuration.
#[derive(Debug, Clone)]
pub enum VendorTts {
    Scripted(ScriptedTts),
    // T2.5 adds `Volc(VolcTts)`.
}

impl From<ScriptedTts> for VendorTts {
    fn from(sink: ScriptedTts) -> Self {
        Self::Scripted(sink)
    }
}

impl TtsSink for VendorTts {
    fn provider(&self) -> &'static str {
        match self {
            VendorTts::Scripted(sink) => sink.provider(),
        }
    }

    fn model_version(&self) -> String {
        match self {
            VendorTts::Scripted(sink) => sink.model_version(),
        }
    }

    fn set_marks(&mut self, marks: MarkHandle) {
        match self {
            VendorTts::Scripted(sink) => sink.set_marks(marks),
        }
    }

    fn synthesize(
        &mut self,
        text: &str,
        voice: &VoiceRef,
        epoch: u64,
    ) -> Result<TtsStream, StageError> {
        match self {
            VendorTts::Scripted(sink) => sink.synthesize(text, voice, epoch),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::pipeline::budget::Stage;

    #[tokio::test]
    async fn dispatch_forwards_to_the_selected_vendor() {
        let recorded: Arc<Mutex<Vec<Stage>>> = Arc::new(Mutex::new(Vec::new()));
        let mut stt: VendorStt = ScriptedStt::partial_then_final("你", "你好").into();
        stt.set_marks(MarkHandle::new({
            let recorded = recorded.clone();
            move |stage| recorded.lock().unwrap().push(stage)
        }));
        assert_eq!(stt.provider(), "scripted");
        let mut stream = stt.start(1).expect("starts");
        assert_eq!(stream.next_partial().await.expect("partial").text, "你");
        assert!(stream.next_partial().await.expect("final").committed);
        assert_eq!(
            recorded.lock().unwrap().as_slice(),
            [Stage::SttFirstPartial]
        );

        let mut translator: VendorTranslator = ScriptedTranslator::one_fragment("Hello.").into();
        assert_eq!(translator.provider(), "scripted");
        let fragment = ZhFragment {
            text: "你好。".to_string(),
            seq: 1,
        };
        let mut translated = translator.translate(&fragment, &[], 1).expect("translates");
        assert!(matches!(
            translated.next().await,
            Some(TranslatorEvent::Fragment { .. })
        ));

        let mut tts: VendorTts = ScriptedTts::default().into();
        assert_eq!(tts.model_version(), "scripted-1");
        let voice = VoiceRef::Preset("zh_female_vv_uranus_bigtts".to_string());
        let mut audio = tts.synthesize("Hello.", &voice, 1).expect("synthesises");
        assert!(audio.next_audio().await.is_some());
    }
}
