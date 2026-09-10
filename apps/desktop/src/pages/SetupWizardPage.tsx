import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faCheck, faCircleNotch } from '@fortawesome/free-solid-svg-icons';
import NeobrutalismButton from '../components/NeobrutalismButton';
import Skeleton from '../components/Skeleton';
import WizardShell from '../components/WizardShell';
import { MOCK_BADGE_LABEL, MOCK_DETECTION_ITEMS, SIM_SOURCE_BADGE_LABEL } from '../data/mock-data';

const STEPS = ['欢迎', '安装 BlackHole 说明', '检测与权限', '完成'] as const;

/** UI-only phase: the probe resolves after a short skeleton beat (Phase 3 does
 *  the real device + permission checks). */
const DETECT_DELAY_MS = 600;

const INSTALL_STEPS = [
  '打开随应用附带的 BlackHole 2ch.pkg 安装包。',
  '按提示输入管理员密码，完成驱动安装后重启电脑。',
  '在 系统设置 → 声音 中，把输出设备切换为 BlackHole 2ch。',
];

/**
 * SetupWizardPage (引导向导) — 4 steps per the Missing Pages Contract.
 * UI only: the install step shows the guided-install copy block rather than
 * running an installer, and the detection step reports simulated results
 * marked 模拟数据.
 */
export default function SetupWizardPage() {
  const navigate = useNavigate();
  const [step, setStep] = useState(0);
  const [detecting, setDetecting] = useState(false);
  const [detected, setDetected] = useState(false);
  const timerRef = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    },
    [],
  );

  const runDetection = () => {
    setDetecting(true);
    setDetected(false);
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      setDetecting(false);
      setDetected(true);
    }, DETECT_DELAY_MS);
  };

  const badge = (
    <span className="shrink-0 rounded-full border-2 border-black bg-rickBlue px-2 py-0.5 text-[10px] font-bold text-black">
      {SIM_SOURCE_BADGE_LABEL}
    </span>
  );

  return (
    <WizardShell
      title="引导向导"
      steps={STEPS}
      current={step}
      onBack={() => navigate('/console')}
      badge={badge}
      onPrev={() => setStep((value) => Math.max(0, value - 1))}
      actions={
        step === STEPS.length - 1 ? (
          <NeobrutalismButton onClick={() => navigate('/console')} className="w-full">
            完成
          </NeobrutalismButton>
        ) : (
          <NeobrutalismButton onClick={() => setStep((value) => value + 1)} className="w-full">
            下一步
          </NeobrutalismButton>
        )
      }
    >
      {step === 0 ? (
        <div className="space-y-3">
          <h2 className="text-[15px] font-bold text-white">欢迎使用极言</h2>
          <p className="text-[13px] leading-relaxed text-gray-400">
            这几步会配置虚拟音频驱动、麦克风权限，并完成一次链路检测。
          </p>
          <section className="rounded-xl border-4 border-black bg-spaceDark p-3">
            <p className="text-[13px] font-bold text-mortyYellow">{SIM_SOURCE_BADGE_LABEL}</p>
            <p className="mt-1 text-[12px] leading-relaxed text-gray-400">
              当前会话的语音与 AI 输出全部来自本地模拟脚本，不会调用任何云端接口，也不需要麦克风以外的硬件。
            </p>
          </section>
        </div>
      ) : null}

      {step === 1 ? (
        <div className="space-y-3">
          <h2 className="text-[15px] font-bold text-white">安装 BlackHole</h2>
          <p className="text-[13px] leading-relaxed text-gray-400">
            极言通过 BlackHole 虚拟音频驱动采集面试官的声音，不需要外接声卡。
          </p>
          <ol className="space-y-2">
            {INSTALL_STEPS.map((line, index) => (
              <li key={line} className="flex gap-2 text-[13px] leading-relaxed text-gray-400">
                <span
                  aria-hidden="true"
                  className="flex h-5 w-5 shrink-0 items-center justify-center rounded bg-mortyYellow text-[10px] font-bold text-black"
                >
                  {index + 1}
                </span>
                <span>{line}</span>
              </li>
            ))}
          </ol>
          <p className="rounded-xl border-2 border-gray-700 bg-spaceDark p-3 text-[12px] leading-relaxed text-gray-400">
            这一步只说明安装方法，安装包与驱动不会由极言自动执行。
          </p>
        </div>
      ) : null}

      {step === 2 ? (
        <div className="space-y-3">
          <h2 className="text-[15px] font-bold text-white">检测与权限</h2>
          <p className="text-[13px] leading-relaxed text-gray-400">
            重新检测，确认下面的环境项是否就绪。
          </p>
          <section
            aria-label="环境检测"
            className="rounded-xl border-4 border-black bg-spaceDark p-3"
          >
            <div className="mb-2 flex items-center justify-between gap-2">
              <span className="text-[10px] font-bold uppercase tracking-wider text-gray-400">
                检测结果
              </span>
              <span className="rounded border-2 border-black bg-mortyYellow px-1.5 py-0.5 text-[10px] font-bold text-black">
                {MOCK_BADGE_LABEL}
              </span>
            </div>
            <ul className="space-y-2">
              {MOCK_DETECTION_ITEMS.map((item) => (
                <li key={item.id} className="flex items-start justify-between gap-2">
                  <span className="min-w-0">
                    <span className="block text-[13px] font-bold text-white">{item.label}</span>
                    <span className="block text-[12px] text-gray-400">{item.detail}</span>
                  </span>
                  {detecting ? (
                    <Skeleton className="h-5 w-14 shrink-0" />
                  ) : (
                    <span
                      className={`flex shrink-0 items-center gap-1 text-[12px] font-bold ${
                        detected ? 'text-portalGreen' : 'text-gray-400'
                      }`}
                    >
                      <FontAwesomeIcon
                        icon={detected ? faCheck : faCircleNotch}
                        aria-hidden="true"
                      />
                      {detected ? '已就绪' : '未检测'}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </section>
          <NeobrutalismButton variant="paper" size="sm" onClick={runDetection} disabled={detecting}>
            重新检测
          </NeobrutalismButton>
        </div>
      ) : null}

      {step === 3 ? (
        <div className="space-y-3">
          <h2 className="text-[15px] font-bold text-white">配置完成</h2>
          <p className="text-[13px] leading-relaxed text-gray-400">
            回到控制台开始一次模拟会话，即可看到双语字幕与 AI 策略的完整链路。
          </p>
          <p className="text-center text-[10px] text-gray-500">C-137</p>
        </div>
      ) : null}
    </WizardShell>
  );
}
