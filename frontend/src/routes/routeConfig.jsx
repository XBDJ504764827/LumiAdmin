import React, { lazy, Suspense } from 'react';
import { PageSkeleton } from '../shared/PageSkeleton.jsx';

const DashboardPage = lazy(() => import('../pages/dashboard/DashboardPage.jsx').then(m => ({ default: m.DashboardPage })));
const CommunityPage = lazy(() => import('../pages/community/CommunityPage.jsx').then(m => ({ default: m.CommunityPage })));
const RconPage = lazy(() => import('../pages/rcon/RconPage.jsx').then(m => ({ default: m.RconPage })));
const WhitelistPage = lazy(() => import('../pages/whitelist/WhitelistPage.jsx').then(m => ({ default: m.WhitelistPage })));
const BanPage = lazy(() => import('../pages/ban/BanPage.jsx').then(m => ({ default: m.BanPage })));
const BanAppealPage = lazy(() => import('../pages/banAppeal/BanAppealPage.jsx').then(m => ({ default: m.BanAppealPage })));
const PlayerReportPage = lazy(() => import('../pages/playerReport/PlayerReportPage.jsx').then(m => ({ default: m.PlayerReportPage })));
const UsersPage = lazy(() => import('../pages/users/UsersPage.jsx').then(m => ({ default: m.UsersPage })));
const LogsPage = lazy(() => import('../pages/logs/LogsPage.jsx').then(m => ({ default: m.LogsPage })));
const ApiListPage = lazy(() => import('../pages/api/ApiListPage.jsx').then(m => ({ default: m.ApiListPage })));
const PlayerApiPage = lazy(() => import('../pages/api/PlayerApiPage.jsx').then(m => ({ default: m.PlayerApiPage })));
const ExternalBanApiPage = lazy(() => import('../pages/api/ExternalBanApiPage.jsx').then(m => ({ default: m.ExternalBanApiPage })));
const ExternalServerPage = lazy(() => import('../pages/external/ExternalServerPage.jsx').then(m => ({ default: m.ExternalServerPage })));
const MapSyncPage = lazy(() => import('../pages/mapSync/MapSyncPage.jsx').then(m => ({ default: m.MapSyncPage })));
const PlayerAccessPage = lazy(() => import('../pages/playerAccess/PlayerAccessPage.jsx').then(m => ({ default: m.PlayerAccessPage })));
const AuditPage = lazy(() => import('../pages/audit/AuditPage.jsx').then(m => ({ default: m.AuditPage })));
const NotificationPage = lazy(() => import('../pages/notifications/NotificationPage.jsx').then(m => ({ default: m.NotificationPage })));
const PublicApplyPage = lazy(() => import('../pages/public/PublicApplyPage.jsx').then(m => ({ default: m.PublicApplyPage })));
const PublicWhitelistPage = lazy(() => import('../pages/public/PublicWhitelistPage.jsx').then(m => ({ default: m.PublicWhitelistPage })));
const PublicBanPage = lazy(() => import('../pages/public/PublicBanPage.jsx').then(m => ({ default: m.PublicBanPage })));
const PublicBanAppealPage = lazy(() => import('../pages/public/PublicBanAppealPage.jsx').then(m => ({ default: m.PublicBanAppealPage })));
const PublicPlayerReportPage = lazy(() => import('../pages/public/PublicPlayerReportPage.jsx').then(m => ({ default: m.PublicPlayerReportPage })));
const NotFoundPage = lazy(() => import('../pages/notFound/NotFoundPage.jsx').then(m => ({ default: m.NotFoundPage })));

function Lazy({ children }) {
  return <Suspense fallback={<PageSkeleton />}>{children}</Suspense>;
}

export const protectedRoutes = [
  { path: '/dashboard', element: <Lazy><DashboardPage /></Lazy>, roles: ['admin', 'developer'] },
  { path: '/community', element: <Lazy><CommunityPage /></Lazy>, roles: ['admin', 'developer', 'normal'] },
  { path: '/rcon', element: <Lazy><RconPage /></Lazy>, roles: ['admin', 'developer'] },
  { path: '/whitelist', element: <Lazy><WhitelistPage /></Lazy>, roles: ['admin', 'developer', 'normal'] },
  { path: '/ban', element: <Lazy><BanPage /></Lazy>, roles: ['admin', 'developer', 'normal'] },
  { path: '/ban-appeal', element: <Lazy><BanAppealPage /></Lazy>, roles: ['admin', 'developer'] },
  { path: '/player-reports', element: <Lazy><PlayerReportPage /></Lazy>, roles: ['admin', 'developer'] },
  { path: '/users', element: <Lazy><UsersPage /></Lazy>, roles: ['admin', 'developer', 'normal'] },
  { path: '/player-access', element: <Lazy><PlayerAccessPage /></Lazy>, roles: ['admin', 'developer', 'normal'] },
  { path: '/audit', element: <Lazy><AuditPage /></Lazy>, roles: ['admin', 'developer', 'normal'] },
  { path: '/notifications', element: <Lazy><NotificationPage /></Lazy>, roles: ['admin', 'developer', 'normal'] },
  { path: '/logs', element: <Lazy><LogsPage /></Lazy>, roles: ['admin', 'developer'] },
  { path: '/docs/api', element: <Lazy><ApiListPage /></Lazy>, roles: ['admin', 'developer'] },
  { path: '/player-api', element: <Lazy><PlayerApiPage /></Lazy>, roles: ['admin', 'developer'] },
  { path: '/external-ban-api', element: <Lazy><ExternalBanApiPage /></Lazy>, roles: ['admin', 'developer'] },
  { path: '/external-servers', element: <Lazy><ExternalServerPage /></Lazy>, roles: ['admin', 'developer'] },
  { path: '/map-sync', element: <Lazy><MapSyncPage /></Lazy>, roles: ['admin', 'developer'] },
];

export const publicRoutes = [
  { path: '/public/apply', element: <Lazy><PublicApplyPage /></Lazy>, roles: ['guest', 'admin', 'developer', 'normal'] },
  { path: '/public/whitelist', element: <Lazy><PublicWhitelistPage /></Lazy>, roles: ['guest', 'admin', 'developer', 'normal'] },
  { path: '/public/ban', element: <Lazy><PublicBanPage /></Lazy>, roles: ['guest', 'admin', 'developer', 'normal'] },
  { path: '/public/ban-appeal', element: <Lazy><PublicBanAppealPage /></Lazy>, roles: ['guest', 'admin', 'developer', 'normal'] },
  { path: '/public/player-report', element: <Lazy><PublicPlayerReportPage /></Lazy>, roles: ['guest', 'admin', 'developer', 'normal'] },
];
