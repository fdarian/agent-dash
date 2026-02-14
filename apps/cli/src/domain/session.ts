export type SessionStatus = 'active' | 'idle';

export interface ClaudeSession {
	paneId: string;
	paneTarget: string;
	title: string;
	sessionName: string;
	status: SessionStatus;
}

const BRAILLE_START = 0x2800;
const BRAILLE_END = 0x28ff;

export function parseSessionStatus(paneTitle: string): SessionStatus {
	if (paneTitle.length === 0) return 'idle';
	const firstChar = paneTitle.codePointAt(0);
	if (firstChar === undefined) return 'idle';
	if (firstChar >= BRAILLE_START && firstChar <= BRAILLE_END) return 'active';
	return 'idle';
}
