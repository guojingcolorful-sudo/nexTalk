// Safari 15.6 floor: Promise.withResolvers is absent in WKWebView (Pitfall 5)
// — polyfill FIRST, before any dependency runs.
import 'core-js/proposals/promise-with-resolvers';
import '@fontsource/space-grotesk/400.css';
import '@fontsource/space-grotesk/600.css';
import '@fontsource/space-grotesk/700.css';
import '@nextalk/design-tokens';
import './styles/global.css';

import React from 'react';
import { createRoot } from 'react-dom/client';
import { HashRouter } from 'react-router-dom';
import App from './App';

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <HashRouter>
      <App />
    </HashRouter>
  </React.StrictMode>,
);
