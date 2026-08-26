# 架构研究：NexTalk 实时 AI 同传面试辅助系统

**领域:** 实时语音翻译 / AI 面试辅助桌面系统（macOS 虚拟声卡 + Tauri 桌面端 + 云端级联流式管线 + 手机 H5 提词器）
**研究日期:** 2026-08-26
**置信度:** HIGH（关键事实均经官方文档/仓库验证）

## 系统总览

真实同传/语音 Agent 类桌面产品的通用结构是 **五层纵向分割 + 两条音频回路**：音频 I/O 层、级联流式管线层、桌面应用层、跨端同步层、AI 辅助引擎层。级联流式（streaming ASR → streaming MT → streaming TTS）仍是 2026 年生产系统的主导范式（胜过端到端模型），典型玻璃到玻璃延迟 800ms–2s，正好命中本项目 ≤2s 目标。

```
┌─────────────────────────────────────────────────────────────────────┐
│  Layer 5: 云端 AI 服务（外部，非本地进程）                              │
│  Deepgram Nova-3/Flux (STT WS)  ·  LLM 流式翻译 (OpenAI-compat)       │
│  ElevenLabs Flash v2.5 (TTS WS)  ·  Tavily (搜索)                    │
└──────────────┬───────────────────────────────┬───────────────────────┘
               │ WS: PCM16 16k  ↑ 文本流/音频流        │ WS: 句子文本 → 音频帧
┌──────────────▼───────────────────────────────▼───────────────────────┐
│  Layer 2: 级联流式管线（Rust, tokio 异步 stage 队列）                    │
│  VAD/分段 → STT stage → 增量翻译 stage → 句子聚合 → TTS stage          │
│  另: 提问结束检测(静默+LLM完整度) → Copilot stage → 策略要点流           │
└───────▲──────────────────────────────────────────────────┬───────────┘
        │ f32 音频帧（ring buffer）                          │ TTS PCM → BlackHole
┌───────┴──────────────────────────────────────────────────▼───────────┐
│  Layer 1: 音频 I/O 层（Rust, 专用实时线程, 与 UI 线程隔离）               │
│  MicCapture(cpal)  ·  LoopbackCapture(cpal@BlackHole)  ·  TtsOutput    │
│  DeviceManager(coreaudio-sys: 多输出设备创建/默认设备切换/路由引导)       │
└───────▲──────────────────────────┬────────────────────────────────────┘
        │                          │
┌───────┴───────────┐   ┌──────────▼──────────────────────────────────┐
│ 用户麦克风(中文)    │   │  BlackHole 2ch (虚拟声卡, HAL plugin)         │
│ → 耳机/多输出       │   │  = Zoom 输入设备（合成英文声）                  │
└───────────────────┘   └──────────────────────────────────────────────┘
                                                         ▲
┌────────────────────────────────────────────────────────┴─────────────┐
│  Layer 3: 桌面应用层（Tauri 2: Rust core + React WebView）              │
│  Tauri commands/events  ·  会话状态  ·  全局快捷键(防抓屏 orderOut)      │
│  虚拟声卡引导向导  ·  本地资产(分轨录音/逐字稿/复盘报告)                   │
│  LAN WebSocket 服务器 + 配对 token + 二维码                              │
└───────────────────────────────┬──────────────────────────────────────┘
                                │ WS (JSON 协议: 字幕/策略/控制/心跳)
┌───────────────────────────────▼──────────────────────────────────────┐
│  Layer 4: 手机 H5 提词器（React, 扫码配对, 上字幕下策略卡）                │
│  全中/全英/双语切换  ·  Wake Lock 防休眠(注意安全上下文限制)              │
└──────────────────────────────────────────────────────────────────────┘
```

## 组件边界（Component Boundaries）

| 组件 | 职责 | 实现方式 | 与谁通信 |
|------|------|----------|----------|
| **DeviceManager** | 枚举设备、检测 BlackHole、创建多输出设备、切换默认输入/输出、路由引导 | `coreaudio-rs`/`CoreAudio-sys`（cpal 无法创建聚合设备） | MicCapture / LoopbackCapture / 设置向导 |
| **MicCapture** | 采集用户中文麦克风 f32 音频帧 | `cpal` input stream，专用音频线程 | Pipeline VAD/STT；分轨录音 |
| **LoopbackCapture** | 采集对方英文原声（从 BlackHole 读回） | `cpal` input stream 目标设备=BlackHole | 对方 STT→中文字幕；分轨录音 |
| **TtsOutput** | 把合成英文 PCM 写入 BlackHole（→Zoom 虚拟麦克风） | `cpal` output stream 目标设备=BlackHole，320ms 块 | TTS stage 输出的音频队列 |
| **STT stage** | 中文流式识别，输出 partial/final 文本 | Deepgram WS（linear16 16k 单声道），8s keepalive | MicCapture；翻译 stage；字幕 |
| **翻译 stage** | 增量翻译（partial 驱动），按句子边界聚合后发给 TTS | LLM 流式 chat（GPT-4o-mini 级别，供应商研究后择优） | STT stage；TTS stage；字幕 |
| **TTS stage** | 英文流式合成（用户克隆音色），句子级流式 | ElevenLabs Flash v2.5 WS（或 Cartesia），sentence-streaming | 翻译 stage；TtsOutput |
| **提问结束检测** | 静默检测(约 800ms) + `speech_final` + LLM 问句完整性判断 | 本地能量 VAD + 规则 + 小 LLM 调用 | Loopback STT；Copilot stage |
| **Copilot stage** | 简历/题库知识库 RAG + 可选 Tavily 搜索 → 流式策略要点 | LLM + 本地向量/关键词检索 + Tavily | 提问结束检测；桌面右栏 + 手机下栏 |
| **Recording 资产** | 双轨录音（我的/对方的）、逐字稿导出、复盘报告 | `hound`(WAV) / `audiopus`(Opus)，本地文件存储 | Mic/Loopback 流；会话状态 |
| **LAN server** | 局域网 WS 服务、配对 token、IP 发现、二维码 | `tokio-tungstenite`（或 axum ws）嵌入 Rust core | 手机 H5；Tauri 前端 |
| **Tauri core** | 命令/事件桥、会话状态、设置、快捷键、防抓屏窗口 | Tauri 2 + tokio + `tauri-plugin-global-shortcut` | WebView 前端；各 layer |
| **桌面前端** | 双栏 UI（左字幕右 AI 策略）、引导向导、资产页 | React + Vite + 设计系统（新粗野主义） | Tauri core（IPC） |
| **手机 H5** | 扫码配对、双语滚动字幕、策略卡片、语言切换 | React + Vite（独立 app，共享协议类型包） | LAN server（WS） |

**边界铁律：** 音频只存在于 Rust 层（Layer 1-2）。WebView/JS 绝不碰音频（浏览器 getUserMedia 管道延迟 300-400ms + 权限问题，且无法访问虚拟设备）。WebView 只消费文本/状态事件。

## 关键架构决策（含置信度）

### 决策 1：虚拟声卡 = 安装原版 BlackHole，应用不自研、不 fork 捆绑 ⚠️ 关键

**事实（HIGH，官方仓库验证）：BlackHole 是 GPL-3.0 许可，不是 MIT。** 官方 README 明确："You can use BlackHole as long as your app is also licensed as GPL-3.0. For all other applications please contact Existential Audio directly."（非 GPLv3 项目需商业授权）。

| 方案 | 代价 | 结论 |
|------|------|------|
| 应用引导向导让用户安装官方 BlackHole（brew/pkg），应用仅检测/使用设备 | 多一步安装引导；驱动与应用分离，无 GPL 污染 | ✅ **v1 采用**——业界常见（Echoless 等即此模式） |
| fork BlackHole 改名捆绑进商业闭源应用 | 需 Existential Audio 商业授权；每个渠道变体 xcodebuild 签名公证 | ❌ 除非拿授权 |
| 从零自写 CoreAudio HAL plugin | `AudioServerPlugInDriverInterface` 是庞大 C API，数月工作量 | ❌ 不可行于 v1 |
| 购买 Rogue Amoeba Loopback（商业闭源） | ~$99/设备，可编程性差，无法随应用分发 | ❌ 不如 BlackHole |

**技术事实（HIGH）：** BlackHole 是 CoreAudio HAL plugin（非内核扩展、非 DriverKit dext），安装到 `/Library/Audio/Plug-Ins/HAL/BlackHoleXch.driver`，实现 `AudioServerPlugInDriverInterface`；核心是 65536 帧 f32 环形缓冲，`kLatency_Frame_Size` 默认 0 → **零附加驱动延迟**；通过预处理宏可配通道数(2/16/64/128/256)与采样率(8k-768k)；需管理员权限安装 + 重启 coreaudiod。分发安装包需 Developer ID 签名 + 公证（其 `create_installer.sh` 即演示此流程）。

### 决策 2：回环拓扑 = 多输出设备（Multi-Output Device），而非纯软件路由

- **出站（合成英文）**：TTS → BlackHole 2ch → 设为 Zoom 的输入设备（虚拟麦克风）。
- **入站（对方原声）**：把 Zoom 输出设备设为 **多输出设备 [耳机 + BlackHole]** → 用户零延迟听到原声（硬件层分发），同时应用从 BlackHole 捕获对方音频做 STT/分析。**无需软件拷贝，耳机透传是零延迟的。**
- 多输出设备可编程创建（HIGH）：`AudioHardwareCreateAggregateDevice()` + `kAudioAggregateDeviceIsStackedKey=true`（stacked 语义=同一输出扇出到所有子设备）。⚠️ 私有聚合设备不能设为默认输出；聚合内所有设备采样率必须一致（统一 48kHz）。
- 回声消除（AEC）大部分被拓扑消解：TTS 只进 BlackHole 不进扬声器 + 用户戴耳机 → 无扬声器→麦克风回声路径；Zoom 自身对虚拟输入做 AEC。残余场景（用户开外放）才需要 WebRTC AEC。

### 决策 3：音频库 = cpal 为主 + coreaudio-rs/CoreAudio-sys 补充

- **cpal**（HIGH）：纯 Rust 跨平台，macOS 上栈为 cpal → coreaudio-rs → CoreAudio-sys。实测 10-20ms 采集延迟，下限 ~5ms。**macOS 上 cpal stream 非 `Send + Sync`** → 流必须跑在专用长生命周期音频线程，经 mpsc/crossbeam channel 把样本送进 tokio 异步管线；用 `Arc<AtomicBool>` 做暂停标志，避免频繁销毁流。
- **cpal 做不了的事**（需 coreaudio-rs/CoreAudio-sys，MEDIUM-HIGH）：创建多输出/聚合设备、`AudioUnitReset`、切换默认设备。
- **audiopus 0.3.x**（HIGH）：libopus 1.3 的 Rust 绑定（songbird 生产使用），用于本地录音压缩（Opus）与可选 STT 传输编码。Deepgram 官方推荐 `linear16` PCM，本地上行直接发 PCM 即可（16k 单声道，1024-8192 字节/块，1024B≈32ms）。
- 采样率统一策略：采集设备原生格式接受，软件层降混单声道 + 重采样到 16k（STT）与 48k（多输出聚合）。

### 决策 4：音频引擎进程内 vs sidecar

**v1 进程内（Rust core 内）**：Tauri 主进程承载 tokio 运行时 + 专用音频线程。理由：少一套进程间协议；Echoless 的 sidecar 模式（JSONL 协议 + GUI 监督）是为崩溃隔离，NexTalk v1 用 panic 兜底（audio 线程 `catch_unwind` + 状态上报）即可。WebView 崩溃不影响 Rust 进程——这是 Tauri 相对 Electron 的结构优势。若后续发现音频线程 panic 频率高，再升级 sidecar 隔离。

### 决策 5：跨端同步 = 单一 WebSocket（非 SSE）

- 手机提词器需要**双向**通道（服务端推字幕/策略/状态；客户端发语言切换/暂停/重连心跳）+ 低延迟。SSE 只适合纯下行，还得另起控制通道——混合方案复杂度高于单一 WS（MEDIUM：StackOverflow 共识；WebSocket 也是移动端最省电的长连接方案）。
- Rust 侧用 `tokio-tungstenite` 嵌入核心，监听 `0.0.0.0:随机高位端口`；配对：桌面生成二维码 `ws://<LAN-IP>:<port>/?token=<随机会话token>`，H5 扫码即连，token 校验通过才可订阅（无认证系统下的最小安全边界）。
- LAN IP 发现：Rust `getifaddrs` 枚举活动接口。H5 同源复用同一 WS 会话，桌面端 UI 与手机看到同一事件流（桌面走 Tauri 内部事件，手机走 WS——统一消息类型）。
- ⚠️ **手机防休眠是个坑**（HIGH）：Screen Wake Lock API 仅 iOS Safari 16.4+ 且**要求安全上下文（HTTPS/localhost）**——`http://192.168.x.x` 不是安全上下文，wake lock 不可用。回退方案：隐藏循环静音 `<video>`（NoSleep.js 手法，无安全上下文要求）；引导用户把自动锁定设长。PWA 安装态在旧 iOS 上 wake lock 也失效（WebKit bug 205104）。

## 推荐项目结构

```
nexTalk/  (pnpm workspace + Cargo workspace)
├── apps/
│   ├── desktop/                 # Tauri 2 + React + Vite（桌面 UI）
│   │   └── src-tauri/
│   │       └── src/
│   │           ├── main.rs / lib.rs      # Tauri builder、plugins、managed state
│   │           ├── audio/
│   │           │   ├── devices.rs        # 枚举/BlackHole检测/多输出创建 (coreaudio-sys)
│   │           │   ├── capture.rs        # cpal 输入流(mic+loopback) + ring buffer
│   │           │   ├── playback.rs       # cpal 输出 → BlackHole
│   │           │   ├── resample.rs       # 48k→16k 单声道、降混
│   │           │   └── record.rs         # 双轨 WAV(hound)/Opus(audiopus)
│   │           ├── pipeline/
│   │           │   ├── source.rs         # AudioSource trait（Mic/Sim/Loopback）
│   │           │   ├── vad.rs            # 能量门 + 静默检测(提问结束)
│   │           │   ├── stt.rs            # Deepgram WS 客户端
│   │           │   ├── translate.rs      # LLM 流式翻译 + 句子聚合
│   │           │   ├── tts.rs            # ElevenLabs/Cartesia WS 客户端
│   │           │   ├── copilot.rs        # 提问结束检测 + KB RAG + 策略流
│   │           │   └── search.rs         # Tavily
│   │           ├── lan/
│   │           │   ├── server.rs         # tokio WS + token 配对 + IP 发现
│   │           │   └── protocol.rs       # JSON 消息类型（与 H5 共享）
│   │           ├── commands/             # Tauri command 处理器（薄层）
│   │           └── state.rs              # SessionState (Arc<RwLock>)
│   └── teleprompter/            # 手机 H5（React + Vite，独立构建）
├── packages/
│   ├── protocol/                # WS 消息类型 + 校验（桌面与 H5 共享）
│   └── design-tokens/           # 新粗野主义设计令牌（两端共享）
└── docs/                        # 引导向导文案、驱动安装脚本
```

**结构理由：**
- `audio/` 与 `pipeline/` 分离：音频是实时线程，管线是异步 stage；边界是 ring buffer / channel。
- `pipeline/source.rs` 的 `AudioSource` trait 是**模拟模式的关键**——UI/手机端可以先跑在 SimSource 上。
- 双前端 + 共享包：协议与设计令牌只写一份，桌面事件流与手机 WS 载荷同构。

## 架构模式（Architectural Patterns）

### 模式 1：级联管线并行（Pipeline Parallelism）

**是什么：** 用异步队列解耦各级，STT 处理第 N 块时翻译处理第 N-1 块、TTS 合成第 N-2 块。IEEE 实测最多 **3.1x 延迟降低**（MEDIUM）。每级有类型化契约：`AudioChunk` → `TranscriptDelta` → `Sentence` → `AudioChunk`。

**何时用：** 一切跨 stage 通信。Rust 侧即 tokio `mpsc` 链，失败级可从队列重试而不重跑前级。

```rust
// 每级 = 一个 tokio task，消费上游 channel，产出下游 channel
tokio::spawn(stt_stage(rx_audio, tx_text));      // 持续收 PCM，发 partial/final
tokio::spawn(translate_stage(rx_text, tx_sent)); // 增量翻译，句子边界聚合
tokio::spawn(tts_stage(rx_sent, tx_pcm));        // 句子级流式合成，写 BlackHole
```

### 模式 2：全链路流式（Streaming Everywhere）

**是什么：** 每一级都必须输出增量结果，绝不等整句：
- STT：Deepgram interim results 每 100-200ms 一推（Nova-3 首次 partial 57ms、Flux 19ms，MEDIUM）；`is_final=true` 只锁词不锁句，须拼接到 `speech_final=true` 才是一个完整语句（HIGH，Deepgram 官方语义）。
- 翻译：partial 文本驱动 LLM 流式输出（wait-k 式策略）；**给 TTS 的必须是完整句子**（句子边界聚合，避免半句怪语调）。
- TTS：**句子级流式**（LLM 出第一个完整句即合成，可比等全文减少 40-60% 首音感知延迟，MEDIUM）；ElevenLabs Flash v2.5 推理 ~75ms（MEDIUM），`buffer_size` 100-150 字符起步出音。

**何时用：** 这是 ≤2s 目标的唯一实现路径。整句串行链路延迟 5-10s，必死。

**权衡：** streaming TTS 比非流式 WER 差 ~2pp（MEDIUM）；interim results 计费激增（Deepgram 按消息计费，interim 会放大请求量，需预算监控）。

### 模式 3：可模拟音频源（Simulation Source）

**是什么：** `AudioSource` trait（Mic / Loopback / Sim），SimSource 按真实节奏回放预录样本或合成文本流。**UI、手机提词器、Copilot、LAN 同步全部可先在模拟数据上开发与演示，不依赖驱动层。**

**何时用：** 项目第 1 阶段。这是把"音频驱动"这一最大未知从开发关键路径上拿下来的唯一手段。

### 模式 4：配对即授权（Pairing-as-Auth）

**是什么：** 桌面生成短期 token 嵌入二维码 URL；WS 握手校验 token，成功后绑会话。无用户体系下最小安全模型。同时用于 Zoom 等会议软件的设备路由引导（向导截图 + 设备名检测）。

### 模式 5：TTS 可打断（Barge-in）

用户再次开口（VAD 触发）→ 取消当前 TTS 合成与播放 → 丢弃剩余 LLM 输出。目标 100ms 内响应（MEDIUM，语音 Agent 行业基准）。Rust 侧 = TTS task 的取消令牌 + 输出流停止标志。

## 数据流（Data Flow）

### 出站流：中文语音 → 英文语音（核心链路，方向明确）

```
用户麦克风 ─f32@48k─▶ MicCapture(音频线程, ring buffer)
  → VAD门控(静音不上行/keepalive) → 降混+重采样16k mono → PCM16块(≈32ms)
  → Deepgram WS ─partial/final 中文文本─▶ 翻译 stage
  → LLM 流式: 英文增量 ──▶ 桌面字幕(中文) + 手机字幕(双语)
  → 句子聚合器(句号/问号/停顿边界) ─完整英文句─▶ TTS stage
  → ElevenLabs WS ─PCM 帧(≈320ms 块)─▶ TtsOutput → BlackHole 2ch → Zoom 虚拟麦克风
```

### 入站流：对方英文 → 耳机（零延迟）+ 中文字幕 + 提问结束检测

```
Zoom 输出 → 多输出设备[耳机 + BlackHole] ─┬─▶ 耳机（硬件直通，零延迟，无软件路径）
                                          └─▶ LoopbackCapture(cpal@BlackHole)
                                              → 对方 STT(英文) → 中文翻译 → 手机字幕
                                              → 静默检测(≈800ms) + speech_final + LLM问句完整性
                                                → 触发 Copilot
```

### Copilot 流：提问结束 → 策略要点

```
提问结束检测 → [简历知识库(PDF/Word解析后本地存储) + 题库预设] 检索
  + 可选 Tavily 实时搜索 → LLM 生成流式要点提纲
  → 桌面右栏卡片 + 手机下栏卡片（同一事件流，不同渲染）
```

### 录音资产流

```
MicCapture / LoopbackCapture 旁路 → 双轨 WAV/Opus（时间戳对齐，对方轨需合规提示）
  → 会话结束 → 逐字稿(双语文案对齐导出) → 复盘报告(LLM: Action Items/情绪/关注点)
```

### 状态管理

- Rust 侧 `SessionState`（Arc<RwLock>）为唯一事实源：连接状态、各 stage 延迟统计、会话时间线。
- WebView 与手机 H5 均为**订阅渲染层**，无业务状态（符合"服务端状态不进客户端 store"）。
- 逐字稿/翻译文本可幂等重放（stage 消费后保留在会话时间线，供重连补齐）。

## 延迟预算（≤2s 目标的分解）

| 段 | 预算 | 实测参考（2026） | 置信度 |
|----|------|------------------|--------|
| 采集→STT 上行 | ~50ms | cpal 10-20ms + 32ms 块 + 网络 | HIGH |
| STT 首个 partial | ~200-400ms | Nova-3 57ms（模型内，端到端加网络）；Flux 19ms | MEDIUM |
| LLM 首个 token | ~200-500ms | 流式 chat 标准 | MEDIUM |
| TTS 首帧 | ~100-300ms | Flash v2.5 推理 ~75ms + buffer 填充 | MEDIUM |
| 播放渲染 | ~20ms | 320ms 块内首帧即播 | HIGH |
| **合计** | **~600ms-1.2s** | 全流式前提下 | MEDIUM |

结论：**≤2s 在 2026 年的供应商组合下全流式即可达成**，无需自研模型；但任何一级退化为非流式（等整句），预算即爆。

## 构建顺序（Build Order）— 对路线图的关键结论

**核心答案：UI/手机端可以先于驱动层建设并演示。** 依赖图决定了阶段顺序：

```
依赖方向：手机 H5 → LAN WS 协议 → 管线事件流 → 音频 I/O → 虚拟设备
          （协议先定，越上层越可 mock）
```

| 阶段 | 内容 | 依赖 | 演示能力 |
|------|------|------|----------|
| **P1 骨架 + 模拟模式** | Tauri 骨架、设计系统、桌面双栏 UI、手机 H5、LAN WS + 二维码配对、快捷键/防抓屏；管线以 SimSource + 假 STT/LLM/TTS 跑通全事件流 | 无 | ✅ 全 UI 演示（模拟音频） |
| **P2 真实云端管线** | cpal 采集默认麦克风、真 Deepgram/LLM/ElevenLabs 流式打通，输出到默认设备（耳听验证） | 无需驱动 | ✅ 真实翻译演示（不走 Zoom） |
| **P3 虚拟声卡集成** | BlackHole 引导安装向导、多输出设备创建、设备路由、TTS→BlackHole、回环捕获 | P2 + 驱动安装 | ✅ 完整 Zoom 闭环 |
| **P4 Copilot 引擎** | 提问结束检测、简历/题库解析入库、RAG、Tavily、策略流 | P2（模拟模式下可提前） | ✅ AI 辅助完整 |
| **P5 本地资产** | 双轨录音、逐字稿、复盘报告 | P2 | ✅ 会后闭环 |

**理由：** P1 定协议与 UI（最大可变风险最低的先行项）；P2 验证**延迟预算**这一核心产品假设（不需要驱动）；P3 引入唯一的外部依赖（驱动安装/签名公证是流程风险，不是架构风险）；P4/P5 是增量价值层。**阶段研究的标记：** P3 的驱动签名/公证流程与向导 UX 值得在 P2 后期预研；P4 的断句误判（提问结束检测）是核心体验风险，需在 P2 阶段就用真实对话样本验证。

## 扩展性考虑（按产品里程碑重述，非用户规模）

| 里程碑 | 结构调整 |
|--------|----------|
| 原型（v1 模拟+真实链路） | 单进程单体，全部进程内 stage |
| 内测（多用户并发会话） | 瓶颈=API 成本（Deepgram interim 计费、ElevenLabs 信用点），加用量监控/限额 |
| 产品化（分发） | 驱动签名公证流程、审计日志（录音合规）、可选 sidecar 隔离、供应商抽象层落地（已留 trait 边界） |

**优先瓶颈：** ① 网络抖动→TTS 播放毛刺（jitter buffer 60-80ms）；② API 成本失控（interim results 放大计费）。两者都是 P2 就该埋监控的。

## 反模式（Anti-Patterns）

### 反模式 1：fork BlackHole 直接捆绑分发
**人们会做：** 看到 MIT 传言/README 可改名（`kDriver_Name` 等宏），直接 fork 改名塞进安装包。
**为什么错：** GPL-3.0——商业闭源应用捆绑即侵权；官方明确非 GPLv3 需单独授权。
**改为：** 引导向导安装官方版本（brew/官方 pkg），应用只做检测 + 路由引导。

### 反模式 2：整句串行链路
**人们会做：** STT 等整句、翻译等整句、TTS 等整句——"先跑通再说"。
**为什么错：** 延迟 5-10s，核心价值（≤2s）直接归零，且 UI 无法迭代。
**改为：** 第一天就按级联流式设计 stage 队列与契约。

### 反模式 3：音频跑在 UI/主线程或阻塞 tokio
**人们会做：** 在 Tauri command 里同步等待 STT 响应、把音频回调接到 WebView。
**为什么错：** macOS 音频回调线程受限 + 阻塞 tokio 会卡死整个命令层；浏览器音频路径延迟 300-400ms。
**改为：** 专用音频线程 + channel；所有 IO 级联为 async task；WebView 只收事件。

### 反模式 4：把 `is_final` 当语句结束
**人们会做：** 收到第一个 `is_final: true` 就触发翻译+合成。
**为什么错：** 长句会产生多个 `is_final: true`（词锁定但人还在说）；缺 `speech_final` 语义则断句错乱。
**改为：** 拼接至 `speech_final: true`；TTS 再经本地句子边界聚合。

### 反模式 5：TTS 播放过度缓冲
**人们会做：** 攒 500ms-1s 音频再播，图"顺滑"。
**为什么错：** 缓冲即延迟地板，直接吃掉预算。
**改为：** 320ms 块起播（NVIDIA 实测甜点，MEDIUM），网络抖动用 60-80ms jitter buffer 而非大缓冲。

### 反模式 6：用透明度动画"防抓屏"
**人们会做：** 把窗口 opacity 降到 0。
**为什么错：** 截屏引擎读窗口缓冲，透明化拦不住捕获（PROJECT.md 已判定）。
**改为：** 窗口真隐藏（orderOut），透明度仅作过渡反馈。

### 反模式 7：忽略 wake lock 安全上下文限制
**人们会做：** 手机 H5 直接调 `navigator.wakeLock.request()`。
**为什么错：** `http://192.168.x.x` 非安全上下文 → API 不存在；iOS Safari 还要求用户手势触发。
**改为：** 特性检测 + "开始提词"按钮触发 + 隐藏循环视频回退 + 引导用户设长自动锁。

## 集成点（Integration Points）

### 外部服务

| 服务 | 集成模式 | 要点 |
|------|----------|------|
| Deepgram STT | WS `wss://api.deepgram.com/v1/listen`，`Authorization: Token <key>` | `encoding=linear16&sample_rate=16000&channels=1&interim_results=true&endpointing=...&vad_events=true`；8s keepalive 防断连；1024-8192B 块；Nova-3 已支持 `zh`（含流式，MEDIUM-HIGH）；注意 interim 计费放大 |
| LLM 翻译 | OpenAI-compat 流式 chat（供应商研究后择优，v1 可 4o-mini 级） | partial 驱动；系统提示词固定翻译策略（技术面试术语表）；句子边界由流式输出文本判断 |
| ElevenLabs TTS | WS 流式（Flash v2.5），长连复用省 cold-start | 句子级流式 + `buffer_size≈100-150` 字符；克隆：Instant VC 1-3 分钟干净音频（v1 够用），Professional VC 30 分钟+ 才接近人声；备选 Cartesia Sonic（首帧 <50ms，更便宜，MEDIUM） |
| Tavily | REST，Copilot 触发时调用 | 异步，不阻塞主线；结果进 LLM 上下文 |

### 内部边界

| 边界 | 通信 | 注意事项 |
|------|------|----------|
| 音频线程 ↔ 管线 | 有界 mpsc + ring buffer | 背压：管线慢时丢帧提示而非阻塞音频回调 |
| 管线 stage ↔ stage | tokio mpsc，类型化消息 | 每级可 mock/替换（供应商抽象） |
| Rust core ↔ WebView | Tauri `invoke`/`emit`-`listen` | 高频字幕事件用事件流，低频控制用 command |
| Rust core ↔ 手机 | WS JSON（packages/protocol 定义） | token 配对；断线重连 + 会话时间线重放 |
| Rust core ↔ 文件系统 | 本地资产目录（应用数据目录） | 分轨录音合规提示（对方同意），纯本地不传云 |

## Sources

- [BlackHole 官方仓库（GPL-3.0 许可声明）](https://github.com/ExistentialAudio/BlackHole)
- [BlackHole 技术架构（HAL plugin/环形缓冲/零延迟）](https://deepwiki.com/ExistentialAudio/BlackHole/4-technical-architecture)
- [BlackHole 多输出与聚合设备](https://deepwiki.com/ExistentialAudio/BlackHole/3.2-multi-output-and-aggregate-devices)
- [BlackHole 构建/签名/公证系统](https://deepwiki.com/ExistentialAudio/BlackHole/5.1-build-system)
- [程序化创建多输出设备（kAudioAggregateDeviceIsStackedKey）](https://stackoverflow.com/questions/35469569/how-can-i-programmatically-create-a-multi-output-device-in-os-x)
- [SimplyCoreAudio（Swift CoreAudio 封装）](https://github.com/secretmissionsoftware/SimplyCoreAudio)
- [BlackHole+多输出组合实践](https://github.com/ranfysvalle02/mac-audio-hackery)
- [cpal 文档（固定缓冲/设备枚举/格式协商）](https://docs.rs/cpal)（Context7 验证）
- [cpal macOS 采样丢失调试与 AudioUnitReset 限制](https://debamitro.github.io/blog/mystery-of-missing-audio-samples-ii/)
- [rustdesk 音频服务（Opus LowDelay/静音门控模式）](https://deepwiki.com/rustdesk/rustdesk/5.3-audio-service)
- [audiopus crate（libopus 绑定）](https://docs.rs/audiopus)
- [Tauri 2 官方文档（架构/IPC/插件）](https://v2.tauri.app/)
- [Tauri 平台支持与 WebView 版本](https://tauri.app/ko/reference/webview-versions/)
- [Deepgram 实时端点化与 interim 语义（is_final vs speech_final）](https://developers.deepgram.com/docs/understand-endpointing-interim-results)
- [Deepgram Nova-3 亚太语言支持（含普通话流式）](https://deepgram.com/learn/deepgram-nova-3-expands-speech-to-text-support-across-asia-pacific)
- [实时语音到语音翻译架构指南（Deepgram）](https://deepgram.com/learn/real-time-speech-to-speech-translation)
- [语音 Agent 延迟预算公开基准](https://orbit.devotel.io/en/benchmarks/latency)
- [ElevenLabs vs Cartesia 2026 流式 TTS 对比](https://futureagi.com/blog/elevenlabs-vs-cartesia-tts-2026/)
- [ElevenLabs 语音克隆 2026 评测](https://www.coval.ai/blog/elevenlabs-review-2026-voice-cloning-and-synthesis-capabilities-explained)
- [SSE vs WebSocket（移动端实时推送取舍）](https://stackoverflow.com/posts/25547739/revisions)
- [Screen Wake Lock API 兼容性（iOS Safari 16.4+）](https://caniuse.com/wake-lock)
- [iOS 真机 Wake Lock 实测与回退方案](https://zenn.dev/ww24/articles/d111b4a80079a0)
- [cascaded 管线并行 3.1x 延迟降低（IEEE 研究，经二次转引）](https://www.forasoft.com/learn/real-time-speech-translation-live-video)

---
*架构研究：NexTalk 实时 AI 同传面试辅助系统*
*研究日期: 2026-08-26*
