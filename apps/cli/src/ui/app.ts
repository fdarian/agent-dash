import { BoxRenderable, createCliRenderer, type KeyEvent } from '@opentui/core';
import { Effect, Fiber, Ref, Schedule } from 'effect';
import type { ClaudeSession, SessionStatus } from '../domain/session.ts';
import { TmuxClient } from '../services/tmux-client.ts';
import { createPanePreview } from './pane-preview.ts';
import { createSessionList } from './session-list.ts';
import { createHelpOverlay } from './help-overlay.ts';

export const App = Effect.gen(function* () {
	const tmux = yield* TmuxClient;

	const renderer = yield* Effect.promise(() =>
		createCliRenderer({
			exitOnCtrlC: true,
			targetFps: 60,
		}),
	);

	const root = new BoxRenderable(renderer, {
		id: 'root',
		flexDirection: 'row',
		width: '100%',
		height: '100%',
	});
	renderer.root.add(root);

	const helpOverlay = createHelpOverlay(renderer);
	renderer.root.add(helpOverlay.backdrop);
	renderer.root.add(helpOverlay.modal);

	const sessionList = createSessionList(renderer);
	const panePreview = createPanePreview(renderer);
	root.add(sessionList.box);
	root.add(panePreview.box);

	const sessionsRef = yield* Ref.make<Array<ClaudeSession>>([]);
	const selectedIndexRef = yield* Ref.make(0);
	const focusRef = yield* Ref.make<'sessions' | 'preview'>('sessions');
	const prevStatusMapRef = yield* Ref.make<Map<string, SessionStatus>>(new Map());
	const unreadPaneIdsRef = yield* Ref.make<Set<string>>(new Set());

	const refreshSessionListUI = Effect.gen(function* () {
		const sessions = yield* Ref.get(sessionsRef);
		const selectedIndex = yield* Ref.get(selectedIndexRef);
		const focus = yield* Ref.get(focusRef);

		const sessionsFocused = focus === 'sessions';
		const previewFocused = focus === 'preview';

		if (sessionsFocused) {
			root.remove(panePreview.box.id);
			root.add(sessionList.box);
			root.add(panePreview.box);
		} else {
			root.remove(sessionList.box.id);
		}

		sessionList.setFocused(sessionsFocused);
		panePreview.setFocused(previewFocused);

		const unreadPaneIds = yield* Ref.get(unreadPaneIdsRef);
		sessionList.update(sessions, selectedIndex, unreadPaneIds);
	});

	const refreshPreviewUI = Effect.gen(function* () {
		const sessions = yield* Ref.get(sessionsRef);
		const selectedIndex = yield* Ref.get(selectedIndexRef);

		if (sessions.length > 0 && selectedIndex < sessions.length) {
			const selected = sessions[selectedIndex];
			if (selected !== undefined) {
				const content = yield* tmux
					.capturePaneContent(selected.paneTarget)
					.pipe(
						Effect.catchAll(() => Effect.succeed('(unable to capture pane)')),
					);
				panePreview.update(content);
			} else {
				panePreview.update('');
			}
		} else {
			panePreview.update('');
		}
	});

	const pollSessions = Effect.gen(function* () {
		const sessions = yield* tmux.discoverSessions.pipe(
			Effect.catchAll(() => Effect.succeed([] as Array<ClaudeSession>)),
		);
		yield* Ref.set(sessionsRef, sessions);

		const selectedIndex = yield* Ref.get(selectedIndexRef);
		if (selectedIndex >= sessions.length && sessions.length > 0) {
			yield* Ref.set(selectedIndexRef, sessions.length - 1);
		}

		const prevStatusMap = yield* Ref.get(prevStatusMapRef);
		const unreadPaneIds = yield* Ref.get(unreadPaneIdsRef);
		const nextStatusMap = new Map<string, SessionStatus>();
		const nextUnreadPaneIds = new Set(unreadPaneIds);
		const currentPaneIds = new Set<string>();

		for (const session of sessions) {
			currentPaneIds.add(session.paneId);
			nextStatusMap.set(session.paneId, session.status);
			const prevStatus = prevStatusMap.get(session.paneId);
			if (prevStatus === 'active' && session.status === 'idle') {
				nextUnreadPaneIds.add(session.paneId);
			}
		}

		for (const paneId of nextUnreadPaneIds) {
			if (!currentPaneIds.has(paneId)) {
				nextUnreadPaneIds.delete(paneId);
			}
		}

		yield* Ref.set(prevStatusMapRef, nextStatusMap);
		yield* Ref.set(unreadPaneIdsRef, nextUnreadPaneIds);

		yield* refreshSessionListUI;
	});

	const pollPreview = Effect.gen(function* () {
		yield* refreshPreviewUI;
	});

	const sessionsFiber = yield* pollSessions.pipe(
		Effect.repeat(Schedule.fixed('2 seconds')),
		Effect.fork,
	);
	const previewFiber = yield* pollPreview.pipe(
		Effect.repeat(Schedule.fixed('200 millis')),
		Effect.fork,
	);

	yield* Effect.sync(() => {
		(renderer.keyInput as unknown as NodeJS.EventEmitter).on(
			'keypress',
			(key: KeyEvent) => {
				const handler = Effect.gen(function* () {
					if (helpOverlay.getIsVisible()) {
						if (helpOverlay.getIsFilterActive()) {
							if (key.name === 'escape') {
								key.preventDefault();
								helpOverlay.hideFilter();
							}
						} else {
							if (key.name === '?' || key.name === 'escape') {
								key.preventDefault();
								helpOverlay.hide();
							} else if (key.name === '/') {
								key.preventDefault();
								helpOverlay.showFilter();
							}
						}
						return;
					}

					const sessions = yield* Ref.get(sessionsRef);
					const selectedIndex = yield* Ref.get(selectedIndexRef);
					const focus = yield* Ref.get(focusRef);

					if (key.name === '1') {
						yield* Ref.set(focusRef, 'sessions');
						yield* refreshSessionListUI;
					} else if (key.name === '0') {
						yield* Ref.set(focusRef, 'preview');
						yield* refreshSessionListUI;
					} else if (key.name === 'j' || key.name === 'down') {
						if (focus === 'sessions') {
							if (selectedIndex < sessions.length - 1) {
								const newIndex = selectedIndex + 1;
								yield* Ref.set(selectedIndexRef, newIndex);
								const newSelected = sessions[newIndex];
								if (newSelected !== undefined) {
									yield* Ref.update(unreadPaneIdsRef, (set) => {
										const next = new Set(set);
										next.delete(newSelected.paneId);
										return next;
									});
								}
								yield* refreshSessionListUI;
								yield* refreshPreviewUI;
							}
						} else if (focus === 'preview') {
							panePreview.scrollBy(1);
						}
					} else if (key.name === 'k' || key.name === 'up') {
						if (focus === 'sessions') {
							if (selectedIndex > 0) {
								const newIndex = selectedIndex - 1;
								yield* Ref.set(selectedIndexRef, newIndex);
								const newSelected = sessions[newIndex];
								if (newSelected !== undefined) {
									yield* Ref.update(unreadPaneIdsRef, (set) => {
										const next = new Set(set);
										next.delete(newSelected.paneId);
										return next;
									});
								}
								yield* refreshSessionListUI;
								yield* refreshPreviewUI;
							}
						} else if (focus === 'preview') {
							panePreview.scrollBy(-1);
						}
					} else if (key.name === 'r') {
						if (sessions.length > 0 && selectedIndex < sessions.length) {
							const selected = sessions[selectedIndex];
							if (selected !== undefined) {
								yield* Ref.update(unreadPaneIdsRef, (set) => {
									const next = new Set(set);
									next.delete(selected.paneId);
									return next;
								});
								yield* refreshSessionListUI;
							}
						}
					} else if (key.name === 'o') {
						if (sessions.length > 0 && selectedIndex < sessions.length) {
							const selected = sessions[selectedIndex];
							if (selected !== undefined) {
								yield* tmux.switchToPane(selected.paneTarget).pipe(
									Effect.catchAll(() => Effect.void),
								);
							}
						}
					} else if (key.name === '?') {
						helpOverlay.toggle();
					} else if (key.name === 'q') {
						renderer.destroy();
					}
				});

				Effect.runPromise(handler).catch(() => {});
			},
		);
	});

	renderer.start();

	yield* Effect.async<void>((resume) => {
		(renderer as unknown as NodeJS.EventEmitter).on('destroy', () => {
			resume(Effect.void);
		});
	});

	yield* Fiber.interrupt(sessionsFiber);
	yield* Fiber.interrupt(previewFiber);
});
