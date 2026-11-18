import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './App';
import { BackendProvider } from './components/backend';
import { getBackend } from './lib/api';
import { useApp, type View } from './lib/store';
import './styles/global.css';
import './styles/views.css';

void getBackend().then((backend) => {
  // The browser demo accepts ?view=review etc. (used for screenshots).
  const requested = new URLSearchParams(window.location.search).get('view') as View | null;
  if (backend.isDemo && requested) {
    const originalInit = useApp.getState().init;
    useApp.setState({
      init: async () => {
        await originalInit();
        useApp.setState({ view: requested });
      },
    });
  }

  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <BackendProvider value={backend}>
        <App />
      </BackendProvider>
    </StrictMode>,
  );
});
