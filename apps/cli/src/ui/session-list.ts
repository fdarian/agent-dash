import { BoxRenderable, type CliRenderer, TextRenderable } from '@opentui/core';
import type { ClaudeSession } from '../domain/session.ts';
import { PRIMARY_COLOR, UNFOCUSED_COLOR, UNREAD_COLOR } from './constants.ts';

export function createSessionList(renderer: CliRenderer) {
	const box = new BoxRenderable(renderer, {
		id: 'session-list',
		width: 40,
		flexDirection: 'column',
		border: true,
		title: '[1] Sessions',
		paddingX: 1,
	});

	let childIds: Array<string> = [];

	function setFocused(focused: boolean) {
		box.borderColor = focused ? PRIMARY_COLOR : UNFOCUSED_COLOR;
	}

	function update(sessions: Array<ClaudeSession>, selectedIndex: number, unreadPaneIds: Set<string>) {
		for (const id of childIds) {
			box.remove(id);
		}
		childIds = [];

		for (let i = 0; i < sessions.length; i++) {
			const session = sessions[i];
			if (session === undefined) continue;

			const isSelected = i === selectedIndex;
			const isUnread = unreadPaneIds.has(session.paneId);

			const iconInfo = session.status === 'active'
				? { icon: '●', defaultFg: PRIMARY_COLOR }
				: isUnread
					? { icon: '◉', defaultFg: UNREAD_COLOR }
					: { icon: '○', defaultFg: '#AAAAAA' };

			const id = `session-item-${i}`;

			const text = new TextRenderable(renderer, {
				id,
				content: `${iconInfo.icon} ${session.title || session.sessionName}`,
				fg: isSelected ? '#FFFFFF' : iconInfo.defaultFg,
				bg: isSelected ? '#444444' : undefined,
			});

			box.add(text);
			childIds.push(id);
		}

		if (sessions.length === 0) {
			const emptyId = 'session-empty';
			const text = new TextRenderable(renderer, {
				id: emptyId,
				content: 'No Claude sessions found',
				fg: '#666666',
			});
			box.add(text);
			childIds.push(emptyId);
		}
	}

	setFocused(true);

	return { box, update, setFocused };
}
