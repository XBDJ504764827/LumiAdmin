// ─────────────────────────────────────────────────────────
// LumiAdmin Global State — Zustand + immer
// ─────────────────────────────────────────────────────────
//
// 替代原来的 React Context + useState 方案：
//   - auth → useAuthStore
//   - theme → useThemeStore
//
// 页面统一从本模块获取 auth/theme hooks，避免 Context 包装层和
// Zustand store 混用。

import { create } from 'zustand';
import { immer } from 'zustand/middleware/immer';
import { api } from '../lib/api.js';
import { defaultSessionFromToken, normalizeSession, readStoredToken } from '../shared/authClient.js';
import {
  getStorage,
  getStoredTheme,
  getSystemTheme,
  persistTheme,
  getNextTheme,
} from './themeCore.js';

// ── 工具：安全获取 localStorage / window ──
const getStorageRef = () => {
  try { return typeof window !== 'undefined' ? window.localStorage : null; } catch { return null; }
};
const getWindow = () => (typeof window !== 'undefined' ? window : null);

// ═══════════════════════════════════════════════════════════
// Theme Store
// ═══════════════════════════════════════════════════════════
const storedTheme = getStoredTheme(getStorage(getWindow()));
const initialTheme = storedTheme ?? getSystemTheme(getWindow());

export const useThemeStore = create((set) => ({
  theme: initialTheme,

  setTheme: (nextTheme) => {
    if (typeof document !== 'undefined') {
      document.documentElement.dataset.theme = nextTheme;
    }
    persistTheme(getStorageRef(), nextTheme);
    set({ theme: nextTheme });
  },

  toggleTheme: () => {
    const nextTheme = getNextTheme(useThemeStore.getState().theme);
    if (typeof document !== 'undefined') {
      document.documentElement.dataset.theme = nextTheme;
    }
    persistTheme(getStorageRef(), nextTheme);
    set({ theme: nextTheme });
  },
}));

// ═══════════════════════════════════════════════════════════
// Auth Store
// ═══════════════════════════════════════════════════════════
// 模块加载时同步读取 token：无 token 的用户首次渲染即为就绪状态，
// 不会先闪一下“正在加载...”的白底界面；有 token 的用户才需要等 api.me 校验。
const _initialToken = readStoredToken(getStorageRef());

export const useAuthStore = create(
  immer((set) => ({
    session: defaultSessionFromToken(_initialToken),
    bootstrapLoading: !!_initialToken,

    initialize() {
      const token = readStoredToken(getStorageRef());
      if (!token) {
        set({ session: null, bootstrapLoading: false });
        return;
      }
      api
        .me(token)
        .then((payload) => {
          set((state) => {
            state.session = normalizeSession(payload);
            state.bootstrapLoading = false;
          });
        })
        .catch(() => {
          if (getStorageRef()) {
            getStorageRef().removeItem('manger_token');
          }
          set({ session: null, bootstrapLoading: false });
        });
    },

    login: async (credentials) => {
      const payload = await api.login(credentials);
      const nextSession = normalizeSession(payload);
      if (getStorageRef() && nextSession?.token) {
        getStorageRef().setItem('manger_token', nextSession.token);
      }
      set((state) => {
        state.session = nextSession;
        state.bootstrapLoading = false;
      });
      return nextSession;
    },

    logout: async () => {
      const current = useAuthStore.getState().session;
      if (current?.token) {
        try { await api.logout(current.token); } catch {}
      }
      if (getStorageRef()) {
        getStorageRef().removeItem('manger_token');
      }
      set({ session: null });
    },
  }))
);

// ═══════════════════════════════════════════════════════════
// 页面级 hooks
// ═══════════════════════════════════════════════════════════
export function useAuth() {
  const session = useAuthStore((s) => s.session);
  const bootstrapLoading = useAuthStore((s) => s.bootstrapLoading);
  const login = useAuthStore((s) => s.login);
  const logout = useAuthStore((s) => s.logout);
  return { session, bootstrapLoading, login, logout };
}

export function useTheme() {
  const theme = useThemeStore((s) => s.theme);
  const setTheme = useThemeStore((s) => s.setTheme);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);
  return { theme, setTheme, toggleTheme };
}
