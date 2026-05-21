import React, { createContext, useContext, useEffect, useMemo, useState } from 'react';
import { api } from '../lib/api.js';
import { defaultSessionFromToken, normalizeSession, readStoredToken } from '../shared/authClient.js';

const AuthContext = createContext(null);

export function AuthProvider({ children }) {
  const [session, setSession] = useState(null);
  const [bootstrapLoading, setBootstrapLoading] = useState(true);

  useEffect(() => {
    const token = readStoredToken(typeof window !== 'undefined' ? window.localStorage : null);
    const initialSession = defaultSessionFromToken(token);
    setSession(initialSession);

    if (!token) {
      setBootstrapLoading(false);
      return;
    }

    api.me(token)
      .then((payload) => {
        setSession(normalizeSession(payload));
      })
      .catch(() => {
        if (typeof window !== 'undefined') {
          window.localStorage.removeItem('manger_token');
        }
        setSession(null);
      })
      .finally(() => {
        setBootstrapLoading(false);
      });
  }, []);

  const value = useMemo(() => ({
    session,
    bootstrapLoading,
    login: async (credentials) => {
      const payload = await api.login(credentials);
      const nextSession = normalizeSession(payload);
      if (typeof window !== 'undefined' && nextSession?.token) {
        window.localStorage.setItem('manger_token', nextSession.token);
      }
      setSession(nextSession);
      return nextSession;
    },
    logout: async () => {
      if (session?.token) {
        try {
          await api.logout(session.token);
        } catch {
        }
      }
      if (typeof window !== 'undefined') {
        window.localStorage.removeItem('manger_token');
      }
      setSession(null);
    },
  }), [session, bootstrapLoading]);

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used within AuthProvider');
  return ctx;
}
