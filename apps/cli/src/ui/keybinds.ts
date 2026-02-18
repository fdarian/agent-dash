type KeybindEntry = {
	key: string;
	description: string;
	context: 'global' | 'sessions' | 'preview';
};

const keybinds: Array<KeybindEntry> = [
	{ key: '0', description: 'Focus preview pane', context: 'global' },
	{ key: '1', description: 'Focus session list', context: 'global' },
	{ key: 'j / ↓', description: 'Next session / Scroll down', context: 'sessions' },
	{ key: 'k / ↑', description: 'Previous session / Scroll up', context: 'sessions' },
	{ key: 'h', description: 'Collapse group', context: 'sessions' },
	{ key: 'l', description: 'Expand group', context: 'sessions' },
	{ key: 'o', description: 'Switch to tmux pane', context: 'global' },
	{ key: 'O', description: 'Open pane scrollback in popup', context: 'global' },
	{ key: 'r', description: 'Mark session as read', context: 'sessions' },
	{ key: '?', description: 'Toggle help', context: 'global' },
	{ key: '/', description: 'Filter keybinds', context: 'global' },
	{ key: 'q', description: 'Quit', context: 'global' },
];

export function filterKeybinds(query: string) {
	if (query.length === 0) return keybinds;
	const lower = query.toLowerCase();
	return keybinds.filter(
		(entry) =>
			entry.key.toLowerCase().includes(lower) ||
			entry.description.toLowerCase().includes(lower) ||
			entry.context.toLowerCase().includes(lower),
	);
}
