import { Navigate, Route, Routes } from 'react-router-dom';
import ConsolePage from './pages/ConsolePage';
import DualPanePage from './pages/DualPanePage';
import GlossaryPage from './pages/GlossaryPage';
import RecordingsPage from './pages/RecordingsPage';
import ResumeImportPage from './pages/ResumeImportPage';
import ReviewPage from './pages/ReviewPage';
import SetupWizardPage from './pages/SetupWizardPage';
import VoiceEnrollmentPage from './pages/VoiceEnrollmentPage';

/**
 * Desktop routing — each Tauri window loads index.html#/<route>:
 * console window -> #/console, dual window -> #/dual (per tauri.conf.json).
 *
 * Six missing pages live inside the console window: 引导向导 / 音色注册 /
 * 术语表 / 简历导入 / 录音资产 / 复盘报告 — all real pages, no PageStub left.
 */
export default function App() {
  return (
    <Routes>
      <Route path="/console" element={<ConsolePage />} />
      <Route path="/dual" element={<DualPanePage />} />
      <Route path="/setup" element={<SetupWizardPage />} />
      <Route path="/voice" element={<VoiceEnrollmentPage />} />
      <Route path="/glossary" element={<GlossaryPage />} />
      <Route path="/resume" element={<ResumeImportPage />} />
      <Route path="/recordings" element={<RecordingsPage />} />
      <Route path="/review" element={<ReviewPage />} />
      <Route path="*" element={<Navigate to="/console" replace />} />
    </Routes>
  );
}
