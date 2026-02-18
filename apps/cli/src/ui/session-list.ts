import { BoxRenderable, type CliRenderer, TextRenderable } from '@opentui/core';
import type { VisibleItem } from './session-groups.ts';
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

	function update(visibleItems: Array<VisibleItem>, selectedIndex: number) {
		for (const id of childIds) {
			box.remove(id);
		}
		childIds = [];

		for (let i = 0; i < visibleItems.length; i++) {
			const item = visibleItems[i];
			if (item === undefined) continue;

			const isSelected = i === selectedIndex;
			const id = `session-item-${i}`;

			if (item.kind === 'group-header') {
				const arrow = item.isCollapsed ? '▶' : '▼';
				const statusIcon = item.hasActive ? '●' : item.hasUnread ? '◉' : '○';

				const text = new TextRenderable(renderer, {
					id,
					content: `${arrow} ${statusIcon} ${item.displayName} (${item.sessionCount})`,
					fg: isSelected ? '#FFFFFF' : '#CCCCCC',
					bg: isSelected ? '#444444' : undefined,
				});

				box.add(text);
				childIds.push(id);
			} else {
				const iconInfo = item.session.status === 'active'
					? { icon: '●', defaultFg: PRIMARY_COLOR }
					: item.isUnread
						? { icon: '◉', defaultFg: UNREAD_COLOR }
						: { icon: '○', defaultFg: '#AAAAAA' };

				const text = new TextRenderable(renderer, {
					id,
					content: `  ${iconInfo.icon} ${item.session.title || item.displayName}`,
					fg: isSelected ? '#FFFFFF' : iconInfo.defaultFg,
					bg: isSelected ? '#444444' : undefined,
				});

				box.add(text);
				childIds.push(id);
			}
		}

		if (visibleItems.length === 0) {
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
