Copilot C-137 UI 设计规范 (Design System Spec)

0. 品牌标识 (Brand Identity)

*   **中文名称**：极言
*   **英文名称**：NexTalk (主视觉标识)
*   **品牌寓意**：Next + Talk。极致延迟、极速响应、下一代跨语言沟通外挂。
*   **Logo 视觉规范**：
    *   **字体**：`Space Grotesk Bold`，大写字母 `NEXTALK`。
    *   **字素高亮**：可以将 `X` 字母做特殊颜色处理（如使用核心色**传送门绿 `#97ce4c`** 或 **莫蒂黄 `#fbf061`**），打破纯文字的单调。
*   **超级符号 (Icon)**：采用纯黑色硬边框风格的**闪电 (`fa-bolt`)** 或带有波浪线的**对话气泡 (`fa-wave-square`)**，体现“极速”与“语音流”的概念。
1. 设计理念 (Design Philosophy)
极客与科幻 (Geek & Sci-Fi)：采用高对比度的暗黑背景结合高饱和度荧光色（传送门绿、瑞克蓝、莫蒂黄），展现黑客工具与外挂的属性。
新粗野主义 (Neobrutalism)：舍弃柔和的阴影和渐变，大量使用粗黑边框 (4px) 和 纯黑硬阴影 (x:4, y:4)。这种风格在视觉上极具冲击力，能让用户在高度紧张的面试中迅速聚焦核心信息。
信息降噪 (Information Denoising)：在手机端取消巨大的交互按钮，改为沉浸式全屏阅读，为高频对话和 AI 提纲留出最大的可视区域。
2. 色彩规范 (Color Palette)
系统采用暗黑主题基底，配以三种高饱和度的主题色用于区分不同的业务逻辑。
2.1 核心品牌与功能色 (Primary Colors)
颜色名称	角色分类	HEX 色值	Tailwind 变量	使用场景说明
传送门绿	用户自我 / 成功 / 发音	#97ce4c	bg-portalGreen	用户的中文输入、AI 克隆的英文输出、麦克风激活状态、核心操作按钮。
莫蒂黄	AI 辅助 / 警示 / 策略	#fbf061	bg-mortyYellow	AI 触发的思考、AI 提供的回答提纲卡片、重要知识库标识。
瑞克蓝	系统提示 / 翻译补充	#00b5cc	bg-rickBlue	面试官原文的中文翻译文本、系统防抓屏提示卡片。
2.2 灰度与背景色 (Neutral & Background Colors)
颜色名称	HEX 色值	Tailwind 变量	使用场景说明
太空深黑	#151519	bg-darkerSpace	桌面端主视窗背景、手机端主背景。
次级深黑	#1E1E24	bg-spaceDark	桌面端侧边栏、内部容器卡片背景。
对话气泡灰	#1E293B	bg-slate-800	对方（面试官）的对话气泡背景色。
纯黑色	#000000	bg-black	边框 (border-black)、硬阴影、文字高对比度背景。
纯白色	#FFFFFF	bg-white	AI 核心策略卡片背景、高亮文本。
3. 字体规范 (Typography)
全站统一使用 Google 免费开源字体 Space Grotesk。该字体具有几何感和轻微的怪异感，非常契合极客外挂的主题。
字重配置：
Regular (400) - 用于次要描述文本。
SemiBold (600) - 用于对话流主文本。
Bold (700) - 核心字重。用于标题、按钮、UI组件文本（本项目大量使用加粗以配合粗黑边框）。
3.1 字号层级 (Type Scale)
场景	Tailwind Class	尺寸 / 行高	大小写 (Case)
桌面端面板标题	text-lg font-bold tracking-wider	18px / 28px	UPPERCASE (大写)
手机端对话原文	text-[15px] font-semibold	15px / 1.5	正常 (Sentence case)
翻译文本 / 备注	text-[13px] font-bold	13px / 1.5	正常
标签 / 状态提示	text-xs font-bold tracking-wider	12px / 16px	UPPERCASE (大写)
迷你徽章 (Toggle)	text-[10px] font-bold	10px / 14px	正常
4. 布局与尺寸规范 (Layout & Dimensions)
4.1 容器尺寸 (Container Sizes)
桌面端微型控制台 (Desktop Widget): 340px (宽) × 680px (高)。用于静默运行时的状态管理。
桌面端扩展视窗 (Desktop Dual-Pane): 860px (宽) × 680px (高)。左半部分（字幕）与右半部分（AI 辅助）各占 50%，无缝并排展示。
移动端视窗 (Mobile App Screen): 参考 iPhone 13/14 尺寸，390px (宽) × 844px (高)。全屏沉浸式。
4.2 间距系统 (Spacing System)
基于 Tailwind 的默认 4px 乘数系统：
页面外边距 (Page Padding): p-4 (16px) 或 p-5 (20px)。
卡片内边距 (Card Padding): 统一使用 p-3 (12px) 或 p-4 (16px)。
元素间距 (Gap): 对话气泡之间使用 gap-6 (24px)，紧密元素使用 gap-2 (8px)。
5. 核心组件规范 (UI Components)
5.1 新粗野主义风格基础 (Neobrutalism Basics)
系统内所有的卡片、按钮、主要视窗，必须遵循以下 CSS 规则：
边框: border-4 border-black (4px 纯黑实线边框)。
圆角: 根据大小分为 rounded-xl (12px), rounded-3xl (24px) 甚至移动端主容器的 rounded-[40px]。
阴影 (Box Shadow): 不使用模糊阴影，使用硬偏移。
黑色主阴影：shadow-[4px_4px_0_0_#000]
彩色高亮阴影：shadow-[4px_4px_0_0_#97ce4c] (绿), #00b5cc (蓝), #fbf061 (黄)。
5.2 交互按钮 (Action Buttons)
默认态: 带有 4px 黑边和 4px 黑色硬阴影。
悬浮/点击态 (Hover/Active): 产生“按压”的物理效果。阴影消失，元素向右下角偏移。
Tailwind 实现: hover:translate-y-1 hover:shadow-none transition-all。
5.3 语言切换器 (Language Toggle)
用于在对话气泡右上角切换单语/双语。
外层容器: flex bg-black border-2 border-gray-600 rounded text-[10px]
未选中态: 文字灰色 text-gray-400。
选中态: 文字白色或黑色，背景填充对应品牌色 (如灰色 bg-gray-600 或绿色 bg-portalGreen)。
5.4 对话流气泡 (Chat Bubbles)
面试官 (Interviewer):
气泡主体背景为深灰 bg-slate-800，圆角去除左上角 rounded-tl-none。
翻译文本紧贴下方，背景略深 bg-slate-700，去边框，文字为瑞克蓝。
用户 (You):
气泡主体背景为暗绿 bg-green-900，边框为传送门绿 border-portalGreen，圆角去除右上角 rounded-tr-none。
5.5 AI 时间轴 (AI Timeline Flow)
AI Copilot 的对话流必须通过左侧引导线串联。
实现方式: 外层容器使用相对定位，添加伪元素画线。
relative before:content-[''] before:absolute before:left-[15px] before:top-[30px] before:bottom-0 before:w-1 before:bg-gray-700
图标节点: 在引导线上放置 32x32px 的圆形 Icon，区分上下文(灰)、策略(黄)、建议(绿)。
5.6 手机端状态胶囊 (Status Capsule)
替代传统的巨大按钮，悬浮于手机屏幕底部。
外观: 黑色深色背景 bg-[#1A1A22]，胶囊形 rounded-full，带黑边和阴影。
内部: 左侧放置红色的呼吸圆点 (animate-pulse) 代表麦克风收音状态，右侧放置辅助功能图标。
6. 交互与状态规范 (States & Animations)
加载/倾听中 (Listening/Generating):
打字机动画: 使用三个连续的圆点，配合延迟的 animate-bounce 动画 (delay: 0s, 0.1s, 0.2s) 模拟 AI 正在思考。
脉冲呼吸灯: 正在录音或监听的提示符（如红色的 REC），必须添加 animate-pulse。
Stealth Mode (防抓屏潜行模式):
快捷键触发: 当用户按下 Cmd + Shift + H，桌面端视窗透明度需在 0.1s 内降为 0 并脱离窗口层级（防止截图引擎捕获），但底层音频服务必须持续运行。
7. 资源库规范 (Assets Library)
图标库 (Icons): 统一使用 FontAwesome 6 Free。
常用图标: fa-microphone (麦克风), fa-brain (AI 辅助), fa-closed-captioning (字幕), fa-lightbulb (策略/提示)。
背景纹理 (Background Texture):
网页底层背景需采用圆点矩阵 (radial-gradient)，增强极客风格和空间感。
CSS 示例: background-image: radial-gradient(#4a4a5c 1px, transparent 1px); background-size: 20px 20px;
文档版本：V1.0 | 状态：已确认 | 目标使用平台：Web/Electron/Tauri/移动端 H5
