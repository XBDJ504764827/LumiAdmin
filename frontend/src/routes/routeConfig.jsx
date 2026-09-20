import { lazy, Suspense } from 'react';
import { PageSkeleton } from '../shared/PageSkeleton.jsx';
import { ErrorBoundary } from '../shared/ErrorBoundary.jsx';
import { ROUTE_ROLES } from './roles.js';
import { hasPermission } from '../shared/permissions.js';

const DashboardPage = lazy(() => import('../pages/dashboard/DashboardPage.jsx').then(m => ({ default: m.DashboardPage })));
const CommunityPage = lazy(() => import('../pages/community/CommunityPage.jsx').then(m => ({ default: m.CommunityPage })));
const RconPage = lazy(() => import('../pages/rcon/RconPage.jsx').then(m => ({ default: m.RconPage })));
const WhitelistPage = lazy(() => import('../pages/whitelist/WhitelistPage.jsx').then(m => ({ default: m.WhitelistPage })));
const BanPage = lazy(() => import('../pages/ban/BanPage.jsx').then(m => ({ default: m.BanPage })));
const AbnormalRecordPage = lazy(() => import('../pages/abnormalRecord/AbnormalRecordPage.jsx').then(m => ({ default: m.AbnormalRecordPage })));
const UsersPage = lazy(() => import('../pages/users/UsersPage.jsx').then(m => ({ default: m.UsersPage })));
const LogsPage = lazy(() => import('../pages/logs/LogsPage.jsx').then(m => ({ default: m.LogsPage })));
const OpsOverviewPage = lazy(() => import('../pages/ops/OpsOverviewPage.jsx').then(m => ({ default: m.OpsOverviewPage })));
const LumiBotStatusPage = lazy(() => import('../pages/ops/LumiBotStatusPage.jsx').then(m => ({ default: m.LumiBotStatusPage })));
const ApiListPage = lazy(() => import('../pages/api/ApiListPage.jsx').then(m => ({ default: m.ApiListPage })));
const PlayerApiPage = lazy(() => import('../pages/api/PlayerApiPage.jsx').then(m => ({ default: m.PlayerApiPage })));
const ExternalBanApiPage = lazy(() => import('../pages/api/ExternalBanApiPage.jsx').then(m => ({ default: m.ExternalBanApiPage })));
const ExternalServerPage = lazy(() => import('../pages/external/ExternalServerPage.jsx').then(m => ({ default: m.ExternalServerPage })));
const AccessLogPage = lazy(() => import('../pages/accessLog/AccessLogPage.jsx').then(m => ({ default: m.AccessLogPage })));
const GlobalBanPage = lazy(() => import('../pages/globalBan/GlobalBanPage.jsx').then(m => ({ default: m.GlobalBanPage })));
const PlayerDetailPage = lazy(() => import('../pages/playerDetail/PlayerDetailPage.jsx').then(m => ({ default: m.PlayerDetailPage })));
const AuditPage = lazy(() => import('../pages/audit/AuditPage.jsx').then(m => ({ default: m.AuditPage })));
const NotificationPage = lazy(() => import('../pages/notifications/NotificationPage.jsx').then(m => ({ default: m.NotificationPage })));
const PublicApplyPage = lazy(() => import('../pages/public/PublicApplyPage.jsx').then(m => ({ default: m.PublicApplyPage })));
const PublicWhitelistPage = lazy(() => import('../pages/public/PublicWhitelistPage.jsx').then(m => ({ default: m.PublicWhitelistPage })));
const PublicBanPage = lazy(() => import('../pages/public/PublicBanPage.jsx').then(m => ({ default: m.PublicBanPage })));
const PublicBanAppealPage = lazy(() => import('../pages/public/PublicBanAppealPage.jsx').then(m => ({ default: m.PublicBanAppealPage })));
const PublicPlayerReportPage = lazy(() => import('../pages/public/PublicPlayerReportPage.jsx').then(m => ({ default: m.PublicPlayerReportPage })));
const PublicMapFeedbackPage = lazy(() => import('../pages/public/PublicMapFeedbackPage.jsx').then(m => ({ default: m.PublicMapFeedbackPage })));

function Lazy({ children }) {
  return (
    <ErrorBoundary>
      <Suspense fallback={<PageSkeleton />}>{children}</Suspense>
    </ErrorBoundary>
  );
}

export function canViewRoute(route, session) {
  return route.roles.includes(session?.role) && (!route.permission || hasPermission(session, route.permission));
}

export const protectedRoutes = [
  { path: '/dashboard', element: <Lazy><DashboardPage /></Lazy>, roles: ROUTE_ROLES.admin, permission: 'community.view' },
  { path: '/community', element: <Lazy><CommunityPage /></Lazy>, roles: ROUTE_ROLES.staff, permission: 'community.view' },
  { path: '/rcon', element: <Lazy><RconPage /></Lazy>, roles: ROUTE_ROLES.staff, permission: 'community.rcon' },
  { path: '/whitelist', element: <Lazy><WhitelistPage /></Lazy>, roles: ROUTE_ROLES.staff, permission: 'whitelist.view' },
  { path: '/ban', element: <Lazy><BanPage /></Lazy>, roles: ROUTE_ROLES.staff, permission: 'ban.view' },
  { path: '/player-detail', element: <Lazy><PlayerDetailPage /></Lazy>, roles: ROUTE_ROLES.staff, permission: 'player_internal.view' },
  { path: '/abnormal-records', element: <Lazy><AbnormalRecordPage /></Lazy>, roles: ROUTE_ROLES.admin, permission: 'player_internal.manage' },
  { path: '/users', element: <Lazy><UsersPage /></Lazy>, roles: ROUTE_ROLES.staff, permission: 'users.view_self' },
  { path: '/access-logs', element: <Lazy><AccessLogPage /></Lazy>, roles: ROUTE_ROLES.admin, permission: 'access_logs.view' },
  { path: '/global-bans', element: <Lazy><GlobalBanPage /></Lazy>, roles: ROUTE_ROLES.admin, permission: 'ban.manage' },
  { path: '/audit', element: <Lazy><AuditPage /></Lazy>, roles: ROUTE_ROLES.staff, permission: 'audit.view' },
  { path: '/notifications', element: <Lazy><NotificationPage /></Lazy>, roles: ROUTE_ROLES.staff, permission: 'community.view' },
  { path: '/logs', element: <Lazy><LogsPage /></Lazy>, roles: ROUTE_ROLES.admin, permission: 'audit.view' },
  { path: '/ops', element: <Lazy><OpsOverviewPage /></Lazy>, roles: ROUTE_ROLES.admin, permission: 'community.view' },
  { path: '/lumi-bot', element: <Lazy><LumiBotStatusPage /></Lazy>, roles: ROUTE_ROLES.admin, permission: 'community.view' },
  { path: '/docs/api', element: <Lazy><ApiListPage /></Lazy>, roles: ROUTE_ROLES.admin, permission: 'community.view' },
  { path: '/player-api', element: <Lazy><PlayerApiPage /></Lazy>, roles: ROUTE_ROLES.admin, permission: 'player_api.manage' },
  { path: '/external-ban-api', element: <Lazy><ExternalBanApiPage /></Lazy>, roles: ROUTE_ROLES.admin, permission: 'ban.manage' },
  { path: '/external-servers', element: <Lazy><ExternalServerPage /></Lazy>, roles: ROUTE_ROLES.admin, permission: 'community.manage' },
];

export const publicRoutes = [
  { path: '/public/apply', element: <Lazy><PublicApplyPage /></Lazy>, roles: ROUTE_ROLES.public },
  { path: '/public/whitelist', element: <Lazy><PublicWhitelistPage /></Lazy>, roles: ROUTE_ROLES.public },
  { path: '/public/ban', element: <Lazy><PublicBanPage /></Lazy>, roles: ROUTE_ROLES.public },
  { path: '/public/ban-appeal', element: <Lazy><PublicBanAppealPage /></Lazy>, roles: ROUTE_ROLES.public },
  { path: '/public/player-report', element: <Lazy><PublicPlayerReportPage /></Lazy>, roles: ROUTE_ROLES.public },
  { path: '/public/map-feedback', element: <Lazy><PublicMapFeedbackPage /></Lazy>, roles: ROUTE_ROLES.public },
];
