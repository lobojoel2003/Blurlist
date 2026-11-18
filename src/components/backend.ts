import { createContext, useContext } from 'react';

import type { Backend } from '../lib/api';

const BackendContext = createContext<Backend | null>(null);
export const BackendProvider = BackendContext.Provider;

export function useBackend(): Backend {
  const b = useContext(BackendContext);
  if (!b) throw new Error('BackendProvider is missing');
  return b;
}
