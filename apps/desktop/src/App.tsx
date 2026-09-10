import { Navigate, Route, Routes } from 'react-router-dom';
import ConsolePage from './pages/ConsolePage';
import DualPanePage from './pages/DualPanePage';
import PageStub from './components/PageStub';

/**
 * Desktop routing — each Tauri window loads index.html#/<route>:
 * console window -> #/console, dual window -> #/dual (per tauri.conf.json).
 *
 * The six missing pages (引导向导 / 音色注册 / 术语表 / 简历导入 / 录音资产 /
 * 复盘报告) render PageStub placeholders for now; Tasks 3-4 replace each one
 * with its real implementation.
 */
export default function App() {
  return (
    <Routes>
      <Route path="/console" element={<ConsolePage />} />
      <Route path="/dual" element={<DualPanePage />} />
      <Route path="/setup" element={<PageStub title="引导向导" />} />
      <Route path="/voice" element={<PageStub title="音色注册" />} />
      <Route path="/glossary" element={<PageStub title="术语表" />} />
      <Route path="/resume" element={<PageStub title="简历导入" />} />
      <Route path="/recordings" element={<PageStub title="录音资产" />} />
      <Route path="/review" element={<PageStub title="复盘报告" />} />
      <Route path="*" element={<Navigate to="/console" replace />} />
    </Routes>
  );
}
