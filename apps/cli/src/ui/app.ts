import { BoxRenderable, createCliRenderer, type KeyEvent } from '@opentui/core';
import { Effect, Fiber, Ref, Schedule } from 'effect';
import * as fs from 'node:fs';
import type { ClaudeSession, SessionStatus } from '../domain/session.ts';
import { TmuxClient } from '../services/tmux-client.ts';
import { createPanePreview } from './pane-preview.ts';
import { createSessionList } from './session-list.ts';
import { createHelpOverlay } from './help-overlay.ts';
import { loadState, saveState } from '../services/state.ts';
import { detectTerminalBackground } from '../utils/terminal.ts';
import { groupSessionsByName, buildVisibleItems, resolveSelectedIndex, type VisibleItem } from './session-groups.ts';

export const App = Effect.gen(function* () {
	const terminalBg = yield* detectTerminalBackground;

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

	const helpOverlay = createHelpOverlay(renderer, terminalBg);
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
	const collapsedGroupsRef = yield* Ref.make<Set<string>>(new Set());
	const visibleItemsRef = yield* Ref.make<Array<VisibleItem>>([]);
	const pipePaneFileRef = yield* Ref.make<string | null>(null);
	const previousPaneTargetRef = yield* Ref.make<string | null>(null);

	const persistedState = yield* loadState;
	yield* Ref.set(prevStatusMapRef, persistedState.prevStatusMap);
	yield* Ref.set(unreadPaneIdsRef, persistedState.unreadPaneIds);

	const getVisibleItems = Effect.gen(function* () {
		const sessions = yield* Ref.get(sessionsRef);
		const collapsedGroups = yield* Ref.get(collapsedGroupsRef);
		const unreadPaneIds = yield* Ref.get(unreadPaneIdsRef);
		const groups = groupSessionsByName(sessions);
		return buildVisibleItems(groups, collapsedGroups, unreadPaneIds);
	});

	const getSelectedSession = Effect.gen(function* () {
		const visibleItems = yield* Ref.get(visibleItemsRef);
		const selectedIndex = yield* Ref.get(selectedIndexRef);
		const item = visibleItems[selectedIndex];
		if (item !== undefined && item.kind === 'session') {
			return item.session;
		}
		return undefined;
	});

	const refreshSessionListUI = Effect.gen(function* () {
		const visibleItems = yield* getVisibleItems;
		yield* Ref.set(visibleItemsRef, visibleItems);
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

		sessionList.update(visibleItems, selectedIndex);
	});

	const refreshPreviewUI = Effect.gen(function* () {
		const selected = yield* getSelectedSession;
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
	});

	const setupPipePane = (paneTarget: string) =>
		Effect.gen(function* () {
			const previousTarget = yield* Ref.get(previousPaneTargetRef);
			if (previousTarget !== null) {
				yield* tmux.stopPipePane(previousTarget).pipe(
					Effect.catchAll(() => Effect.void),
				);
			}

			const tmpFile = yield* tmux.startPipePane(paneTarget).pipe(
				Effect.catchAll(() => Effect.succeed(null as string | null)),
			);
			yield* Ref.set(pipePaneFileRef, tmpFile);
			yield* Ref.set(previousPaneTargetRef, paneTarget);
			yield* refreshPreviewUI;
		});

	const cleanupPipePane = Effect.gen(function* () {
		const previousTarget = yield* Ref.get(previousPaneTargetRef);
		if (previousTarget !== null) {
			yield* tmux.stopPipePane(previousTarget).pipe(
				Effect.catchAll(() => Effect.void),
			);
		}
		const tmpFile = yield* Ref.get(pipePaneFileRef);
		if (tmpFile !== null) {
			yield* Effect.try(() => fs.unlinkSync(tmpFile)).pipe(
				Effect.catchAll(() => Effect.void),
			);
		}
		yield* Ref.set(pipePaneFileRef, null);
		yield* Ref.set(previousPaneTargetRef, null);
	});

	const pollSessions = Effect.gen(function* () {
		const sessions = yield* tmux.discoverSessions.pipe(
			Effect.catchAll(() => Effect.succeed([] as Array<ClaudeSession>)),
		);
		yield* Ref.set(sessionsRef, sessions);

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
		yield* saveState(nextUnreadPaneIds, nextStatusMap);

		const oldVisibleItems = yield* Ref.get(visibleItemsRef);
		const oldSelectedIndex = yield* Ref.get(selectedIndexRef);
		const newVisibleItems = yield* getVisibleItems;
		const newSelectedIndex = resolveSelectedIndex(newVisibleItems, oldVisibleItems, oldSelectedIndex);
		yield* Ref.set(selectedIndexRef, newSelectedIndex);

		yield* refreshSessionListUI;
	});

	const startPreviewWatcher = Effect.gen(function* () {
		const selected = yield* getSelectedSession;
		if (selected !== undefined) {
			yield* setupPipePane(selected.paneTarget);
		}
	});

	yield* startPreviewWatcher;

	const sessionsFiber = yield* pollSessions.pipe(
		Effect.repeat(Schedule.fixed('2 seconds')),
		Effect.fork,
	);

	const fileWatcherFiber = yield* Effect.async<never>((resume) => {
		let debounceTimer: ReturnType<typeof setTimeout> | null = null;
		let currentWatcher: fs.FSWatcher | null = null;
		let watchedPath: string | null = null;

		const startWatching = () => {
			const tmpFile = Effect.runSync(Ref.get(pipePaneFileRef));
			if (tmpFile === null) return;

			try {
				if (!fs.existsSync(tmpFile)) {
					fs.writeFileSync(tmpFile, '');
				}
				currentWatcher = fs.watch(tmpFile, () => {
					if (debounceTimer !== null) clearTimeout(debounceTimer);
					debounceTimer = setTimeout(() => {
						Effect.runPromise(refreshPreviewUI).catch(() => {});
					}, 100);
				});
				watchedPath = tmpFile;
			} catch {
				// File watch failed, fallback poll will handle it
			}
		};

		startWatching();

		const checkInterval = setInterval(() => {
			const tmpFile = Effect.runSync(Ref.get(pipePaneFileRef));
			if (currentWatcher !== null) {
				if (watchedPath !== tmpFile) {
					currentWatcher.close();
					currentWatcher = null;
					watchedPath = null;
					startWatching();
				}
			} else if (tmpFile !== null) {
				startWatching();
			}
		}, 500);

		return Effect.sync(() => {
			if (debounceTimer !== null) clearTimeout(debounceTimer);
			if (currentWatcher !== null) currentWatcher.close();
			clearInterval(checkInterval);
		});
	}).pipe(Effect.fork);

	const fallbackPreviewFiber = yield* refreshPreviewUI.pipe(
		Effect.repeat(Schedule.fixed('5 seconds')),
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

					const visibleItems = yield* Ref.get(visibleItemsRef);
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
							if (selectedIndex < visibleItems.length - 1) {
								yield* Ref.set(selectedIndexRef, selectedIndex + 1);
								yield* refreshSessionListUI;
								const selectedAfterJ = yield* getSelectedSession;
								if (selectedAfterJ !== undefined) {
									yield* setupPipePane(selectedAfterJ.paneTarget);
								}
							}
						} else if (focus === 'preview') {
							panePreview.scrollBy(1);
						}
					} else if (key.name === 'k' || key.name === 'up') {
						if (focus === 'sessions') {
							if (selectedIndex > 0) {
								yield* Ref.set(selectedIndexRef, selectedIndex - 1);
								yield* refreshSessionListUI;
								const selectedAfterK = yield* getSelectedSession;
								if (selectedAfterK !== undefined) {
									yield* setupPipePane(selectedAfterK.paneTarget);
								}
							}
						} else if (focus === 'preview') {
							panePreview.scrollBy(-1);
						}
					} else if (key.name === 'h') {
						if (focus === 'sessions') {
							const currentItem = visibleItems[selectedIndex];
							if (currentItem !== undefined) {
								if (currentItem.kind === 'group-header') {
									yield* Ref.update(collapsedGroupsRef, (set) => {
										const next = new Set(set);
										next.add(currentItem.sessionName);
										return next;
									});
									yield* refreshSessionListUI;
									yield* refreshPreviewUI;
								} else if (currentItem.kind === 'session') {
									yield* Ref.update(collapsedGroupsRef, (set) => {
										const next = new Set(set);
										next.add(currentItem.groupSessionName);
										return next;
									});
									const updatedVisibleItems = yield* getVisibleItems;
									const headerIndex = updatedVisibleItems.findIndex(
										(item) => item.kind === 'group-header' && item.sessionName === currentItem.groupSessionName,
									);
									if (headerIndex !== -1) {
										yield* Ref.set(selectedIndexRef, headerIndex);
									}
									yield* refreshSessionListUI;
									yield* refreshPreviewUI;
								}
							}
						}
					} else if (key.name === 'l') {
						if (focus === 'sessions') {
							const currentItem = visibleItems[selectedIndex];
							if (currentItem !== undefined && currentItem.kind === 'group-header') {
								yield* Ref.update(collapsedGroupsRef, (set) => {
									const next = new Set(set);
									next.delete(currentItem.sessionName);
									return next;
								});
								yield* refreshSessionListUI;
								yield* refreshPreviewUI;
							}
						}
					} else if (key.name === 'r') {
						const selected = yield* getSelectedSession;
						if (selected !== undefined) {
							yield* Ref.update(unreadPaneIdsRef, (set) => {
								const next = new Set(set);
								next.delete(selected.paneId);
								return next;
							});
							const updatedUnread = yield* Ref.get(unreadPaneIdsRef);
							const currentStatusMap = yield* Ref.get(prevStatusMapRef);
							yield* saveState(updatedUnread, currentStatusMap);
							yield* refreshSessionListUI;
						}
					} else if (key.name === 'o' && key.shift) {
						const selected = yield* getSelectedSession;
						if (selected !== undefined) {
							yield* tmux.openPopup(selected.paneTarget).pipe(
								Effect.catchAll(() => Effect.void),
							);
						}
					} else if (key.name === 'o') {
						const selected = yield* getSelectedSession;
						if (selected !== undefined) {
							yield* tmux.switchToPane(selected.paneTarget).pipe(
								Effect.catchAll(() => Effect.void),
							);
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
	yield* Fiber.interrupt(fileWatcherFiber);
	yield* Fiber.interrupt(fallbackPreviewFiber);
	yield* cleanupPipePane;
});
