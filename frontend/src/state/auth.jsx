import { createContext, useCallback, useContext, useEffect } from 'react';
import { useAuthStore } from './store.js';

const AuthContext = createContext(null);

export function AuthProvider({ children }) {
  // 订阅 Zustand store 的状态
  const session = useAuthStore((s) => s.session);
  const bootstrapLoading = useAuthStore((s) => s.bootstrapLoading);

  // 首次挂载时初始化 auth store（从 localStorage 读取 token 并校验）
  useEffect(() => {
    useAuthStore.getState().initialize();
  }, []);

  const login = useCallback(async (credentials) => {
    return useAuthStore.getState().login(credentials);
  }, []);

  const logout = useCallback(async () => {
    return useAuthStore.getState().logout();
  }, []);

  return (
    <AuthContext.Provider value={{ session, bootstrapLoading, login, logout }}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used within AuthProvider');
  return ctx;
}
