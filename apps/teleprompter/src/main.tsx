// Safari 15.6 floor (macOS 12.7 WKWebView + older phones): core-js polyfill
// must be the FIRST import (research Pitfall 5).
import 'core-js/proposals/promise-with-resolvers';
import '@fontsource/space-grotesk/400.css';
import '@fontsource/space-grotesk/600.css';
import '@fontsource/space-grotesk/700.css';
import '@nextalk/design-tokens';
import './styles/global.css';

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
