import { Navigate, Route, Routes } from 'react-router-dom';
import ConsolePage from './pages/ConsolePage';
import DualPanePage from './pages/DualPanePage';

/**
 * Desktop routing — each Tauri window loads index.html#/<route>:
 * console window -> #/console, dual window -> #/dual (per tauri.conf.json).
 * The 6 missing pages (setup wizard, voice enrollment, glossary, resume
 * import, recording assets, review report) land in 01-03.
 */
export default function App() {
  return (
    <Routes>
      <Route path="/console" element={<ConsolePage />} />
      <Route path="/dual" element={<DualPanePage />} />
      <Route path="*" element={<Navigate to="/console" replace />} />
    </Routes>
  );
}
