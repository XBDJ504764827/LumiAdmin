import React from 'react';
import { DashboardPage } from '../pages/dashboard/DashboardPage.jsx';
import { CommunityPage } from '../pages/community/CommunityPage.jsx';
import { WhitelistPage } from '../pages/whitelist/WhitelistPage.jsx';
import { BanPage } from '../pages/ban/BanPage.jsx';
import { UsersPage } from '../pages/users/UsersPage.jsx';
import { LogsPage } from '../pages/logs/LogsPage.jsx';
import { ApiListPage } from '../pages/api/ApiListPage.jsx';
import { PlayerApiPage } from '../pages/api/PlayerApiPage.jsx';
import { PlayerAccessPage } from '../pages/playerAccess/PlayerAccessPage.jsx';
import { AuditPage } from '../pages/audit/AuditPage.jsx';
import { PublicApplyPage } from '../pages/public/PublicApplyPage.jsx';
import { PublicWhitelistPage } from '../pages/public/PublicWhitelistPage.jsx';
import { PublicBanPage } from '../pages/public/PublicBanPage.jsx';

export const protectedRoutes = [
  { path: '/dashboard', element: <DashboardPage />, roles: ['admin', 'developer'] },
  { path: '/community', element: <CommunityPage />, roles: ['admin', 'developer', 'normal'] },
  { path: '/whitelist', element: <WhitelistPage />, roles: ['admin', 'developer', 'normal'] },
  { path: '/ban', element: <BanPage />, roles: ['admin', 'developer', 'normal'] },
  { path: '/users', element: <UsersPage />, roles: ['admin', 'developer', 'normal'] },
  { path: '/player-access', element: <PlayerAccessPage />, roles: ['admin', 'developer', 'normal'] },
  { path: '/audit', element: <AuditPage />, roles: ['admin', 'developer', 'normal'] },
  { path: '/logs', element: <LogsPage />, roles: ['admin', 'developer'] },
  { path: '/docs/api', element: <ApiListPage />, roles: ['admin', 'developer'] },
  { path: '/player-api', element: <PlayerApiPage />, roles: ['admin', 'developer'] },
];

export const publicRoutes = [
  { path: '/public/apply', element: <PublicApplyPage />, roles: ['guest', 'admin', 'developer', 'normal'] },
  { path: '/public/whitelist', element: <PublicWhitelistPage />, roles: ['guest', 'admin', 'developer', 'normal'] },
  { path: '/public/ban', element: <PublicBanPage />, roles: ['guest', 'admin', 'developer', 'normal'] },
];
