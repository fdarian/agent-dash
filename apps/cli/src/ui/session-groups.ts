import type { ClaudeSession } from '../domain/session.ts';

export type SessionGroup = {
	sessionName: string;
	sessions: Array<ClaudeSession>;
};

export type VisibleItem =
	| { kind: 'group-header'; sessionName: string; sessionCount: number; hasActive: boolean; hasUnread: boolean; isCollapsed: boolean }
	| { kind: 'session'; session: ClaudeSession; groupSessionName: string; isUnread: boolean };

export function groupSessionsByName(sessions: Array<ClaudeSession>): Array<SessionGroup> {
	const map = new Map<string, SessionGroup>();
	for (const session of sessions) {
		let group = map.get(session.sessionName);
		if (group === undefined) {
			group = { sessionName: session.sessionName, sessions: [] };
			map.set(session.sessionName, group);
		}
		group.sessions.push(session);
	}
	return Array.from(map.values());
}

export function buildVisibleItems(
	groups: Array<SessionGroup>,
	collapsedGroups: Set<string>,
	unreadPaneIds: Set<string>,
): Array<VisibleItem> {
	const items: Array<VisibleItem> = [];
	for (const group of groups) {
		const hasActive = group.sessions.some((s) => s.status === 'active');
		const hasUnread = group.sessions.some((s) => unreadPaneIds.has(s.paneId));
		const isCollapsed = collapsedGroups.has(group.sessionName);
		items.push({
			kind: 'group-header',
			sessionName: group.sessionName,
			sessionCount: group.sessions.length,
			hasActive,
			hasUnread,
			isCollapsed,
		});
		if (!isCollapsed) {
			for (const session of group.sessions) {
				items.push({
					kind: 'session',
					session,
					groupSessionName: group.sessionName,
					isUnread: unreadPaneIds.has(session.paneId),
				});
			}
		}
	}
	return items;
}

export function resolveSelectedIndex(
	newItems: Array<VisibleItem>,
	oldItems: Array<VisibleItem>,
	oldIndex: number,
): number {
	const oldItem = oldItems[oldIndex];
	if (oldItem === undefined) {
		return Math.max(0, Math.min(oldIndex, newItems.length - 1));
	}
	if (oldItem.kind === 'session') {
		const found = newItems.findIndex(
			(item) => item.kind === 'session' && item.session.paneId === oldItem.session.paneId,
		);
		if (found !== -1) return found;
	}
	if (oldItem.kind === 'group-header') {
		const found = newItems.findIndex(
			(item) => item.kind === 'group-header' && item.sessionName === oldItem.sessionName,
		);
		if (found !== -1) return found;
	}
	return Math.max(0, Math.min(oldIndex, newItems.length - 1));
}
