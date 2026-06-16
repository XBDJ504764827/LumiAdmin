import { createContext, useContext, useMemo } from 'react';
import { useThemeStore } from './store.js';

const ThemeContext = createContext(null);

export function ThemeProvider({ children }) {
  // theme store 的 setTheme/toggleTheme 已在内部同步 DOM，这里只做订阅
  const theme = useThemeStore((s) => s.theme);
  const setTheme = useThemeStore((s) => s.setTheme);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);

  const value = useMemo(() => ({
    theme,
    setTheme,
    toggleTheme,
  }), [theme, setTheme, toggleTheme]);

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) throw new Error('useTheme must be used inside ThemeProvider');
  return context;
}
