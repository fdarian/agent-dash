import * as Effect from 'effect/Effect';
import { join } from 'path';
import { homedir } from 'os';
import type { SessionStatus } from '../domain/session.ts';

const STATE_DIR = join(homedir(), '.config', 'agent-dash');
const STATE_FILE = join(STATE_DIR, 'state.json');

type PersistedState = {
	unreadPaneIds: Array<string>;
	prevStatusMap: Record<string, SessionStatus>;
};

export const loadState = Effect.gen(function* () {
	const raw = yield* Effect.tryPromise(() => Bun.file(STATE_FILE).text()).pipe(
		Effect.catchAll(() => Effect.succeed(null)),
	);

	if (raw === null) {
		return {
			unreadPaneIds: new Set<string>(),
			prevStatusMap: new Map<string, SessionStatus>(),
		};
	}

	const parsed = yield* Effect.try(() => JSON.parse(raw) as PersistedState).pipe(
		Effect.catchAll(() => Effect.succeed(null)),
	);

	if (parsed === null) {
		return {
			unreadPaneIds: new Set<string>(),
			prevStatusMap: new Map<string, SessionStatus>(),
		};
	}

	return {
		unreadPaneIds: new Set(parsed.unreadPaneIds),
		prevStatusMap: new Map(Object.entries(parsed.prevStatusMap)) as Map<string, SessionStatus>,
	};
});

export function saveState(
	unreadPaneIds: Set<string>,
	prevStatusMap: Map<string, SessionStatus>,
) {
	return Effect.gen(function* () {
		const data: PersistedState = {
			unreadPaneIds: Array.from(unreadPaneIds),
			prevStatusMap: Object.fromEntries(prevStatusMap),
		};

		yield* Effect.tryPromise(async () => {
			const { mkdir } = await import('fs/promises');
			await mkdir(STATE_DIR, { recursive: true });
			await Bun.write(STATE_FILE, JSON.stringify(data));
		}).pipe(Effect.catchAll(() => Effect.void));
	});
}
