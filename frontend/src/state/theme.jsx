import React, { createContext, useContext, useEffect, useMemo, useState } from 'react';
import { getNextTheme, getStorage, getStoredTheme, getSystemTheme, persistTheme } from './themeCore.js';

const ThemeContext = createContext(null);

function getWindow() {
  return typeof window === 'undefined' ? null : window;
}

export function ThemeProvider({ children }) {
  const [storedTheme, setStoredTheme] = useState(() => getStoredTheme(getStorage(getWindow())));
  const theme = storedTheme ?? getSystemTheme(getWindow());

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  const value = useMemo(() => ({
    theme,
    setTheme(nextTheme) {
      setStoredTheme(nextTheme);
      persistTheme(getStorage(getWindow()), nextTheme);
    },
    toggleTheme() {
      const nextTheme = getNextTheme(theme);
      setStoredTheme(nextTheme);
      persistTheme(getStorage(getWindow()), nextTheme);
    },
  }), [theme]);

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) throw new Error('useTheme must be used inside ThemeProvider');
  return context;
}
