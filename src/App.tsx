import { useEffect } from 'react';

import { Sidebar } from './components/Sidebar';
import { Toasts } from './components/Toasts';
import { errorMessage, useApp } from './lib/store';
import { ExportView } from './views/ExportView';
import { ReviewView } from './views/ReviewView';
import { RosterView } from './views/RosterView';
import { ScanView } from './views/ScanView';
import { SettingsView } from './views/SettingsView';

export function App() {
  const { view, init, loaded, toast } = useApp();

  useEffect(() => {
    init().catch((e) => toast(errorMessage(e), 'error'));
  }, [init, toast]);

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        {loaded && (
          <>
            {view === 'roster' && <RosterView />}
            {view === 'scan' && <ScanView />}
            {view === 'review' && <ReviewView />}
            {view === 'export' && <ExportView />}
            {view === 'settings' && <SettingsView />}
          </>
        )}
      </main>
      <Toasts />
    </div>
  );
}
