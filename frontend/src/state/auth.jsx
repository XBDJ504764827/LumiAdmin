import React, { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';
import { api } from '../lib/api.js';
import { defaultSessionFromToken, normalizeSession, readStoredToken } from '../shared/authClient.js';

const AuthContext = createContext(null);

export function AuthProvider({ children }) {
  const [session, setSession] = useState(null);
  const [bootstrapLoading, setBootstrapLoading] = useState(true);
  const sessionRef = useRef(session);
  sessionRef.current = session;

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

  const login = useCallback(async (credentials) => {
    const payload = await api.login(credentials);
    const nextSession = normalizeSession(payload);
    if (typeof window !== 'undefined' && nextSession?.token) {
      window.localStorage.setItem('manger_token', nextSession.token);
    }
    setSession(nextSession);
    return nextSession;
  }, []);

  const logout = useCallback(async () => {
    const current = sessionRef.current;
    if (current?.token) {
      try { await api.logout(current.token); } catch {}
    }
    if (typeof window !== 'undefined') {
      window.localStorage.removeItem('manger_token');
    }
    setSession(null);
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
