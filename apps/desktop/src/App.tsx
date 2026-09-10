import { Navigate, Route, Routes } from 'react-router-dom';
import ConsolePage from './pages/ConsolePage';
import DualPanePage from './pages/DualPanePage';
import GlossaryPage from './pages/GlossaryPage';
import PageStub from './components/PageStub';
import SetupWizardPage from './pages/SetupWizardPage';
import VoiceEnrollmentPage from './pages/VoiceEnrollmentPage';

/**
 * Desktop routing — each Tauri window loads index.html#/<route>:
 * console window -> #/console, dual window -> #/dual (per tauri.conf.json).
 *
 * Six missing pages live inside the console window: 引导向导 / 音色注册 /
 * 术语表 / 简历导入 / 录音资产 / 复盘报告. 简历导入, 录音资产 and 复盘报告
 * still render PageStub until Task 4 swaps them for their real pages.
 */
export default function App() {
  return (
    <Routes>
      <Route path="/console" element={<ConsolePage />} />
      <Route path="/dual" element={<DualPanePage />} />
      <Route path="/setup" element={<SetupWizardPage />} />
      <Route path="/voice" element={<VoiceEnrollmentPage />} />
      <Route path="/glossary" element={<GlossaryPage />} />
      <Route path="/resume" element={<PageStub title="简历导入" />} />
      <Route path="/recordings" element={<PageStub title="录音资产" />} />
      <Route path="/review" element={<PageStub title="复盘报告" />} />
      <Route path="*" element={<Navigate to="/console" replace />} />
    </Routes>
  );
}
